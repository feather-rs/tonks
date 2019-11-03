use hashbrown::HashMap;
use std::hash::Hash;

/// Used to create consecutive `usize` mappings for a given type.
#[derive(Debug, Clone, Derivative)]
pub struct Mappings<K, V>
where
    K: Hash + PartialEq + Eq,
    V: Copy + From<usize>,
{
    counter: usize,
    mappings: HashMap<K, V>,
}

impl<K, V> Default for Mappings<K, V>
where
    K: Hash + PartialEq + Eq,
    V: Copy + From<usize>,
{
    fn default() -> Self {
        Self {
            counter: 0,
            mappings: HashMap::new(),
        }
    }
}

impl<K, V> Mappings<K, V>
where
    K: Hash + PartialEq + Eq,
    V: Copy + From<usize>,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_alloc(&mut self, key: K) -> V {
        if let Some(value) = self.mappings.get(&key) {
            *value
        } else {
            self.mappings.insert(key, V::from(self.counter));
            self.counter += 1;
            (self.counter - 1).into()
        }
    }

    pub fn len(&self) -> usize {
        self.mappings.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn basic() {
        let mut mappings = Mappings::<TypeId, usize>::new();

        assert_eq!(mappings.get_or_alloc(TypeId::of::<usize>()), 0);
        assert_eq!(mappings.get_or_alloc(TypeId::of::<isize>()), 1);
    }
}
