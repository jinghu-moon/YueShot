//! O(1) index reads with bounded leaf copying while snapshots retain a prefix.
use std::{ops::Index, rc::Rc};

const BLOCK_CAPACITY: usize = 256;

#[derive(Clone)]
pub(super) struct IndexVector<T> {
    blocks: Rc<Vec<Rc<[T; BLOCK_CAPACITY]>>>,
    len: usize,
}
impl<T> Default for IndexVector<T> {
    fn default() -> Self {
        Self {
            blocks: Rc::default(),
            len: 0,
        }
    }
}
impl<T> IndexVector<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, ix: usize) -> Option<&T> {
        if ix >= self.len {
            return None;
        }
        self.blocks
            .get(ix / BLOCK_CAPACITY)?
            .get(ix % BLOCK_CAPACITY)
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.blocks
            .iter()
            .flat_map(|block| block.iter())
            .take(self.len)
    }
}
impl<T: Copy + Default> IndexVector<T> {
    pub fn push(&mut self, value: T) {
        let blocks = Rc::make_mut(&mut self.blocks);
        if self.len.is_multiple_of(BLOCK_CAPACITY) {
            blocks.push(Rc::new([T::default(); BLOCK_CAPACITY]));
        }
        Rc::make_mut(blocks.last_mut().unwrap())[self.len % BLOCK_CAPACITY] = value;
        self.len += 1;
    }
}
impl<T> Index<usize> for IndexVector<T> {
    type Output = T;
    fn index(&self, ix: usize) -> &T {
        self.get(ix).expect("masonry index in bounds")
    }
}
