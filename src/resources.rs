//! Efficient storing and loading of resources. Unlike other implementations,
//! we use consecutive `usize`s as resource IDs so that a vector can be
//! used rather than a hash map.

use crate::mappings::Mappings;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::any::TypeId;
use std::cell::UnsafeCell;
use std::iter;

/// ID of a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ResourceId(pub usize);

impl From<usize> for ResourceId {
    fn from(x: usize) -> Self {
        Self(x)
    }
}

lazy_static! {
    /// Mappings from `TypeId`s to `ResourceId`s.
    pub static ref RESOURCE_ID_MAPPINGS: Mutex<Mappings<TypeId, ResourceId>> = Mutex::new(Mappings::new());
}

/// Returns the resource ID corresponding to a given type.
pub fn resource_id_for<T: Resource>() -> ResourceId {
    RESOURCE_ID_MAPPINGS.lock().get_or_alloc(TypeId::of::<T>())
}

pub trait Resource: Send + Sync + mopa::Any + 'static {}

impl<T: Send + Sync + mopa::Any> Resource for T {}

#[allow(clippy::transmute_ptr_to_ref)] // https://github.com/chris-morgan/mopa/issues/11
mod mopafy {
    use super::*;
    mopafy!(Resource);
}

/// Stores resources. Resource borrow access is unchecked,
/// so most functions are unsafe.
pub struct Resources {
    /// Stored resources, accessed by the `ResourceId` index.
    resources: Vec<UnsafeCell<Option<Box<dyn Resource>>>>,
}

unsafe impl Send for Resources {}
unsafe impl Sync for Resources {}

impl Default for Resources {
    fn default() -> Self {
        Self { resources: vec![] }
    }
}

impl Resources {
    /// Creates an empty resource container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a reference to the resource.
    ///
    /// # Panics
    /// Panics if the resource does not exist.
    pub fn get<T: Resource>(&self) -> &T {
        unsafe { self.get_unchecked(resource_id_for::<T>()) }
    }

    /// Returns a mutable reference to the resource.
    ///
    /// # Panics
    /// Panics if the resource does not exist.
    pub fn get_mut<T: Resource>(&mut self) -> &mut T {
        // Safety: borrow rules are enforced through &mut self.
        unsafe { self.get_mut_unchecked(resource_id_for::<T>()) }
    }

    /// Returns a reference to the resource with the given ID.
    ///
    /// # Safety
    /// Borrowing is unchecked, allowing for data races.
    /// Care must be taken to ensure that borrowing rules are followed.
    ///
    /// In addition, the type of the resource being requested must match
    /// the ID. (This is checked in debug mode.)
    pub unsafe fn get_unchecked<T: Resource>(&self, id: ResourceId) -> &T {
        debug_assert_eq!(resource_id_for::<T>(), id);
        ((&*self.resources[id.0].get())
            .as_ref()
            .expect("Failed to fetch resource"))
        .as_ref()
        .downcast_ref()
        .unwrap()
    }

    /// Returns a mutable reference to the resource with the given ID.
    ///
    /// # Safety
    /// Borrowing is unchecked, allowing for data races.
    /// Care must be taken to ensure that borrowing rules are followed.
    ///
    /// In addition, the type of the resource being requested must match
    /// the ID. (This is checked in debug mode.)
    #[allow(clippy::mut_from_ref)] // Function is unsafe: users are responsible for this.
    pub unsafe fn get_mut_unchecked<T: Resource>(&self, id: ResourceId) -> &mut T {
        debug_assert_eq!(resource_id_for::<T>(), id);

        (self.resources[id.0]
            .get()
            .as_mut()
            .unwrap()
            .as_mut()
            .expect("Failed to fetch resource"))
        .as_mut()
        .downcast_mut()
        .unwrap()
    }

    /// Inserts a resource of the given type, replacing
    /// the old resource if it exists.
    pub fn insert<T: Resource>(&mut self, value: T) {
        let id = resource_id_for::<T>();

        if self.resources.len() <= id.0 {
            // Extend resources vector
            self.resources.extend(
                iter::repeat_with(|| UnsafeCell::new(None)).take(id.0 - self.resources.len() + 1),
            );
        }

        self.resources[id.0] = UnsafeCell::new(Some(Box::new(value)));
    }

    /// Inserts a resource if it is absent.
    pub fn insert_if_absent<T: Resource>(&mut self, value: T) {
        let id = resource_id_for::<T>();

        if self.resources.len() <= id.0 {
            // Extend resources vector
            self.resources.extend(
                iter::repeat_with(|| UnsafeCell::new(None)).take(id.0 - self.resources.len() + 1),
            );
        }

        let resource = unsafe { &mut *self.resources[id.0].get() };
        if resource.is_some() {
            return;
        }
        self.resources[id.0] = UnsafeCell::new(Some(Box::new(value)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let mut resources = Resources::new();
        resources.insert(1i32);
        resources.insert(1usize);

        unsafe {
            assert_eq!(resources.get_unchecked::<i32>(ResourceId(0)), &1);
            assert_eq!(resources.get_unchecked::<usize>(ResourceId(1)), &1);
        }
    }
}
