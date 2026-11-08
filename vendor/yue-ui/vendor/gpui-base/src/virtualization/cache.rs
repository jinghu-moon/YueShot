use super::*;

/// Portable value; applications may serialize these fields in their own format.
#[derive(Clone, Debug)]
pub struct VirtualizerMeasurement {
    key: ElementId,
    content_revision: u64,
    extent: Pixels,
}
impl VirtualizerMeasurement {
    pub fn new(key: impl Into<ElementId>, content_revision: u64, extent: Pixels) -> Self {
        Self {
            key: key.into(),
            content_revision,
            extent,
        }
    }
    pub fn key(&self) -> &ElementId {
        &self.key
    }
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub fn extent(&self) -> Pixels {
        self.extent
    }
}

/// Remount cache, distinct from a live immutable geometry snapshot.
/// No entity, scroll handle, owner token or old source is retained.
#[derive(Clone, Debug)]
pub struct VirtualizerCache {
    version: u32,
    axis: Axis,
    layout_revision: u64,
    cross: Pixels,
    gap: Pixels,
    items: Vec<VirtualizerMeasurement>,
    anchor: Option<(ElementId, VirtualizerAnchorPosition)>,
}
impl VirtualizerCache {
    pub fn new(
        axis: Axis,
        layout_revision: u64,
        cross: Pixels,
        gap: Pixels,
        items: Vec<VirtualizerMeasurement>,
    ) -> Self {
        Self {
            version: 2,
            axis,
            layout_revision,
            cross,
            gap,
            items,
            anchor: None,
        }
    }
    pub fn anchor(
        mut self,
        key: impl Into<ElementId>,
        position: VirtualizerAnchorPosition,
    ) -> Self {
        self.anchor = Some((key.into(), position));
        self
    }
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }
    pub fn cross_extent(&self) -> Pixels {
        self.cross
    }
    pub fn gap(&self) -> Pixels {
        self.gap
    }
    pub fn items(&self) -> &[VirtualizerMeasurement] {
        &self.items
    }
    pub fn scroll_anchor(&self) -> Option<(&ElementId, VirtualizerAnchorPosition)> {
        self.anchor.as_ref().map(|(key, position)| (key, *position))
    }
}
impl VirtualizerState {
    pub fn measurement_cache(&self) -> Result<VirtualizerCache, VirtualizerError> {
        let list = self
            .measured
            .as_ref()
            .ok_or(VirtualizerError::WrongBackend)?;
        let cross = self
            .viewport
            .as_ref()
            .map(|v| v.cross)
            .filter(|cross| valid_positive(*cross))
            .ok_or(VirtualizerError::InvalidGeometry(
                "cache requires resolved cross extent",
            ))?;
        let items = list
            .geometry()
            .measured_items()
            .into_iter()
            .map(|(ix, size)| {
                // A received viewport is not evidence of a completed layout.
                if size.width != cross {
                    return Err(VirtualizerError::InvalidGeometry(
                        "cache measurements require matching cross extent",
                    ));
                }
                Ok(VirtualizerMeasurement::new(
                    self.source.key_at(ix),
                    self.source.revision_at(ix),
                    size.height
                        - if ix + 1 < self.source.len() {
                            self.layout.gap
                        } else {
                            px(0.)
                        },
                ))
            })
            .collect::<Result<Vec<_>, VirtualizerError>>()?;
        let mut cache = VirtualizerCache::new(
            self.axis,
            self.layout.revision,
            cross,
            self.layout.gap,
            items,
        );
        cache.anchor = self.anchor.clone();
        Ok(cache)
    }
    /// Restore a controller, typically on remount. Missing/changed keys are skipped;
    /// incompatible layout and malformed records fail before changing geometry.
    /// Existing measurements from another constraint become size hints first.
    /// Live invalidate_items epochs are not portable identities: size-affecting
    /// content must advance source.revision_at, shared typography layout.revision.
    pub fn restore_measurement_cache(
        &mut self,
        cache: &VirtualizerCache,
    ) -> Result<usize, VirtualizerError> {
        let list = self
            .measured
            .as_ref()
            .ok_or(VirtualizerError::WrongBackend)?;
        if cache.version != 2
            || cache.axis != self.axis
            || cache.layout_revision != self.layout.revision
            || cache.gap != self.layout.gap
            || !valid_positive(cache.cross)
            || self
                .viewport
                .as_ref()
                .is_some_and(|v| v.cross != cache.cross)
        {
            return Err(VirtualizerError::InvalidGeometry(
                "incompatible measurement cache",
            ));
        }
        if cache
            .anchor
            .as_ref()
            .is_some_and(|(_, position)| !position.is_valid())
        {
            return Err(VirtualizerError::InvalidGeometry("cache anchor"));
        }
        let mut seen = std::collections::HashSet::new();
        let mut updates = Vec::new();
        for item in &cache.items {
            if !valid_positive(item.extent)
                || !valid_positive(item.extent + self.layout.gap)
                || !seen.insert(item.key.clone())
            {
                return Err(VirtualizerError::InvalidGeometry("cache entry"));
            }
            if let Some(ix) = self.source.index_of(&item.key)
                && self.source.revision_at(ix) == item.content_revision
            {
                updates.push((
                    ix,
                    size(
                        cache.cross,
                        item.extent
                            + if ix + 1 < self.source.len() {
                                self.layout.gap
                            } else {
                                px(0.)
                            },
                    ),
                ));
            }
        }
        // Cold restore cannot preserve exact sizes from an incompatible width.
        if list
            .geometry()
            .measured_items()
            .iter()
            .any(|(_, size)| size.width != cache.cross)
        {
            list.remeasure_items(0..self.source.len());
        }
        list.set_measurement_width(cache.cross);
        list.set_item_sizes(&updates, true);
        self.geometry = Geometry::Measured(list.geometry());
        self.changed_geometry();
        self.anchor = cache
            .anchor
            .clone()
            .filter(|(key, _)| self.source.index_of(key).is_some());
        self.restore_anchor();
        Ok(updates.len())
    }
}
