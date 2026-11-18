use std::fmt;
use std::marker::PhantomData;

pub struct Handle<T> {
    index: u32,
    generation: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    #[must_use]
    pub fn index(self) -> u32 {
        self.index
    }

    #[must_use]
    pub fn generation(self) -> u32 {
        self.generation
    }

    #[must_use]
    pub fn from_parts(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            marker: PhantomData,
        }
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}
impl<T> Eq for Handle<T> {}
impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Handle<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.index, self.generation).cmp(&(other.index, other.generation))
    }
}
impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index, self.generation)
    }
}

enum Slot<T> {
    Occupied { generation: u32, value: T },
    Vacant { generation: u32 },
}

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
    cap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full {
    pub cap: usize,
}

impl<T> Arena<T> {
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            cap,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    pub fn insert(&mut self, value: T) -> Result<Handle<T>, Full> {
        if self.live >= self.cap {
            return Err(Full { cap: self.cap });
        }
        let (index, generation) = if let Some(index) = self.free.pop() {
            let slot = self
                .slots
                .get_mut(index as usize)
                .ok_or(Full { cap: self.cap })?;
            let generation = match slot {
                Slot::Vacant { generation } => *generation,
                Slot::Occupied { .. } => return Err(Full { cap: self.cap }),
            };
            *slot = Slot::Occupied { generation, value };
            (index, generation)
        } else {
            let index = u32::try_from(self.slots.len()).map_err(|_| Full { cap: self.cap })?;
            self.slots.push(Slot::Occupied {
                generation: 0,
                value,
            });
            (index, 0)
        };
        self.live = self.live.saturating_add(1);
        Ok(Handle {
            index,
            generation,
            marker: PhantomData,
        })
    }

    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        match self.slots.get(handle.index as usize)? {
            Slot::Occupied { generation, value } if *generation == handle.generation => Some(value),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        match self.slots.get_mut(handle.index as usize)? {
            Slot::Occupied { generation, value } if *generation == handle.generation => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn contains(&self, handle: Handle<T>) -> bool {
        self.get(handle).is_some()
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let slot = self.slots.get_mut(handle.index as usize)?;
        match slot {
            Slot::Occupied { generation, .. } if *generation == handle.generation => {}
            _ => return None,
        }
        let next = handle.generation.checked_add(1);
        let old = std::mem::replace(
            slot,
            Slot::Vacant {
                generation: next.unwrap_or(u32::MAX),
            },
        );
        if next.is_some() {
            self.free.push(handle.index);
        }
        self.live = self.live.saturating_sub(1);
        match old {
            Slot::Occupied { value, .. } => Some(value),
            Slot::Vacant { .. } => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| match slot {
                Slot::Occupied { generation, value } => Some((
                    Handle {
                        index: u32::try_from(i).ok()?,
                        generation: *generation,
                        marker: PhantomData,
                    },
                    value,
                )),
                Slot::Vacant { .. } => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_handle_goes_stale_even_after_its_slot_is_reused() {
        let mut arena = Arena::with_cap(8);
        let a = arena.insert("a").unwrap();
        assert_eq!(arena.remove(a), Some("a"));
        let b = arena.insert("b").unwrap();
        assert_eq!(a.index(), b.index(), "the slot is reused");
        assert_eq!(
            arena.get(a),
            None,
            "the old handle must not see the new value"
        );
        assert_eq!(arena.get(b), Some(&"b"));
        assert_eq!(arena.remove(a), None);
    }

    #[test]
    fn the_live_cap_is_an_error_not_growth() {
        let mut arena = Arena::with_cap(2);
        let a = arena.insert(1).unwrap();
        arena.insert(2).unwrap();
        assert_eq!(arena.insert(3), Err(Full { cap: 2 }));
        arena.remove(a);
        assert!(arena.insert(3).is_ok(), "freeing one makes room for one");
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.slots(), 2, "reuse, not growth");
    }

    #[test]
    fn reuse_order_is_last_freed_first() {
        let mut arena = Arena::with_cap(8);
        let handles: Vec<_> = (0..4).map(|i| arena.insert(i).unwrap()).collect();
        arena.remove(handles[1]);
        arena.remove(handles[3]);
        assert_eq!(arena.insert(9).unwrap().index(), 3);
        assert_eq!(arena.insert(9).unwrap().index(), 1);
    }
}
