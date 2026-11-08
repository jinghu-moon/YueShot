use super::*;

/// A domain-keyed, size-only update. Content invalidation is a separate operation.
#[derive(Clone, Debug)]
pub struct VirtualizerSizeUpdate {
    key: ElementId,
    extent: Pixels,
}
impl VirtualizerSizeUpdate {
    pub fn new(key: impl Into<ElementId>, extent: Pixels) -> Self {
        Self {
            key: key.into(),
            extent,
        }
    }
    pub fn key(&self) -> &ElementId {
        &self.key
    }
    pub fn extent(&self) -> Pixels {
        self.extent
    }
}

impl VirtualizerState {
    /// Validate a size batch without changing geometry or revisions.
    ///
    /// Grid uses this preflight for its two-axis transaction. Keep the
    /// validation rules identical to `apply_sizes`; the second pass is safe
    /// because all fallible input checks have completed before either axis is
    /// mutated.
    pub(crate) fn validate_size_batch(
        &self,
        updates: &[VirtualizerSizeUpdate],
        _measured: bool,
    ) -> Result<(), VirtualizerError> {
        if self.sizing.kind() == VirtualizerSizingKind::Fixed {
            return Err(VirtualizerError::WrongBackend);
        }
        let mut indexed = Vec::with_capacity(updates.len());
        for update in updates {
            let key_index = self
                .source
                .index_of(&update.key)
                .ok_or(VirtualizerError::MissingKey)?;
            if !valid_positive(update.extent) || !valid_positive(update.extent + self.layout.gap) {
                return Err(VirtualizerError::InvalidGeometry("size update"));
            }
            indexed.push((key_index, update.extent));
        }
        indexed.sort_unstable_by_key(|(index, _)| *index);
        if indexed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(VirtualizerError::InvalidChange);
        }
        Ok(())
    }

    /// Atomic batch of known/externally measured content extents (gap excluded).
    /// Fixed lists deliberately reject per-item sizes. Callers notify their entity once.
    pub fn update_sizes(
        &mut self,
        updates: &[VirtualizerSizeUpdate],
    ) -> Result<(), VirtualizerError> {
        self.apply_sizes(updates, true)
    }
    /// Hints remain unmeasured until native layout confirms them.
    pub fn update_size_hints(
        &mut self,
        updates: &[VirtualizerSizeUpdate],
    ) -> Result<(), VirtualizerError> {
        if self.measured.is_none() {
            return Err(VirtualizerError::WrongBackend);
        }
        self.apply_sizes(updates, false)
    }
    pub fn update_sizes_from_snapshot(
        &mut self,
        snapshot: &VirtualizerSnapshot,
        updates: &[VirtualizerSizeUpdate],
    ) -> Result<(), VirtualizerError> {
        self.validate_snapshot(snapshot)?;
        self.update_sizes(updates)
    }
    fn apply_sizes(
        &mut self,
        updates: &[VirtualizerSizeUpdate],
        measured: bool,
    ) -> Result<(), VirtualizerError> {
        if self.sizing.kind() == VirtualizerSizingKind::Fixed {
            return Err(VirtualizerError::WrongBackend);
        }
        if let [update] = updates {
            return self.apply_single_size(update, measured);
        }
        let mut indexed = Vec::with_capacity(updates.len());
        for update in updates {
            let ix = self
                .source
                .index_of(&update.key)
                .ok_or(VirtualizerError::MissingKey)?;
            if !valid_positive(update.extent) || !valid_positive(update.extent + self.layout.gap) {
                return Err(VirtualizerError::InvalidGeometry("size update"));
            }
            indexed.push((ix, update.extent));
        }
        // The indexed transaction buffer also proves key uniqueness. Keep
        // no-op entries until after duplicate validation, then reuse it for
        // sorted backend submission instead of allocating a HashSet as well.
        indexed.sort_unstable_by_key(|(ix, _)| *ix);
        if indexed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(VirtualizerError::InvalidChange);
        }
        indexed.retain(|(ix, extent)| !self.matches_size(*ix, *extent, measured));
        if indexed.is_empty() {
            return Ok(());
        }
        if let Sizing::Known(tree) = &mut self.sizing.0 {
            tree.update_measurements(&indexed, true);
            let mut geometry = tree.clone();
            geometry.gap = self.layout.gap.into();
            self.geometry = Geometry::Known(geometry);
        } else if let Some(list) = &self.measured {
            let cross = self.viewport.as_ref().map_or(px(0.), |v| v.cross);
            let updates: Vec<_> = indexed
                .into_iter()
                .map(|(ix, extent)| {
                    (
                        ix,
                        size(
                            cross,
                            extent
                                + if ix + 1 < self.source.len() {
                                    self.layout.gap
                                } else {
                                    px(0.)
                                },
                        ),
                    )
                })
                .collect();
            list.set_item_sizes(&updates, measured);
            self.geometry = Geometry::Measured(list.geometry());
        }
        self.changed_geometry();
        self.restore_anchor();
        Ok(())
    }
    fn matches_size(&self, ix: usize, extent: Pixels, measured: bool) -> bool {
        let gap = if self.measured.is_some() && ix + 1 < self.source.len() {
            self.layout.gap
        } else {
            px(0.)
        };
        self.geometry.item(ix).is_some_and(|(_, old, exact)| {
            // Equality uses the backend's stored units, including its f32 gap.
            old == f64::from(extent + gap) && exact == measured
        }) && self.measured.as_ref().is_none_or(|list| {
            list.geometry().item(ix).is_some_and(|item| {
                item.cross_extent() == self.viewport.as_ref().map_or(px(0.), |v| v.cross)
            })
        })
    }
    // One feedback item cannot contain duplicates and needs no indexed Vec.
    // Keep this specialization separate from the multi-item transaction body.
    #[inline(never)]
    fn apply_single_size(
        &mut self,
        update: &VirtualizerSizeUpdate,
        measured: bool,
    ) -> Result<(), VirtualizerError> {
        let ix = self
            .source
            .index_of(&update.key)
            .ok_or(VirtualizerError::MissingKey)?;
        if !valid_positive(update.extent) || !valid_positive(update.extent + self.layout.gap) {
            return Err(VirtualizerError::InvalidGeometry("size update"));
        }
        if self.matches_size(ix, update.extent, measured) {
            return Ok(());
        }
        if let Sizing::Known(tree) = &mut self.sizing.0 {
            tree.update_measurements(&[(ix, update.extent)], true);
            let mut geometry = tree.clone();
            geometry.gap = self.layout.gap.into();
            self.geometry = Geometry::Known(geometry);
        } else if let Some(list) = &self.measured {
            let cross = self.viewport.as_ref().map_or(px(0.), |v| v.cross);
            let extent = update.extent
                + if ix + 1 < self.source.len() {
                    self.layout.gap
                } else {
                    px(0.)
                };
            list.set_item_sizes(&[(ix, size(cross, extent))], measured);
            self.geometry = Geometry::Measured(list.geometry());
        }
        self.changed_geometry();
        self.restore_anchor();
        Ok(())
    }
}
