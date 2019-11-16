//! Type-level query APIs as wrappers over Legion queries.

use crate::system::SystemCtx;
use crate::{ResourceId, Resources, SystemData, SystemDataOutput};
use legion::filter::EntityFilter;
use legion::query::{
    ChunkDataIter, ChunkEntityIter, ChunkViewIter, DefaultFilter, IntoQuery, ReadOnly, View,
};
use legion::world::World;

pub struct Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
{
    inner: legion::query::Query<V, <V as DefaultFilter>::Filter>,
}

impl<'a, V> SystemData<'a> for Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type Output = PreparedQuery<V>;

    fn prepare(&'a mut self) -> Self::Output {
        PreparedQuery {
            query: &mut self.inner,
        }
    }

    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources, _ctx: SystemCtx, _world: &World) -> Self {
        Self { inner: V::query() }
    }
}

/// A `legion::World` wrapper which can be safely passed to systems.
pub struct PreparedWorld {
    world: *const World,
}

assert_impl_all!(World: Send, Sync);
unsafe impl Send for PreparedWorld {}
unsafe impl Sync for PreparedWorld {}

impl<'a> SystemData<'a> for PreparedWorld {
    type Output = Self;

    fn prepare(&'a mut self) -> Self::Output {
        Self { world: self.world }
    }

    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources, _ctx: SystemCtx, world: &World) -> Self {
        Self {
            world: world as *const _,
        }
    }
}

impl<'a> SystemDataOutput<'a> for PreparedWorld {
    type SystemData = PreparedWorld;
}

/// A query which has been prepared for passing to a system.
pub struct PreparedQuery<V>
where
    V: DefaultFilter + for<'v> View<'v>,
{
    query: *mut legion::query::Query<V, <V as DefaultFilter>::Filter>,
}

impl<V> PreparedQuery<V>
where
    V: for<'v> View<'v> + DefaultFilter,
{
    // Implementations "borrowed" from Legion's codebase with a few modifications, licensed under MIT.
    // Don't blame me—I'm not going to write all this!

    /// Gets an iterator which iterates through all chunks that match the query.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[inline]
    pub unsafe fn iter_chunks_unchecked<'b, 'c>(
        &'c mut self,
        world: &PreparedWorld,
    ) -> ChunkViewIter<
        'b,
        'c,
        V,
        <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
    > {
        (&mut *self.query).iter_chunks_unchecked(&*world.world)
    }

    /// Gets an iterator which iterates through all chunks that match the query.
    pub fn iter_chunks_immutable<'b, 'c>(
        &'c mut self,
        world: &PreparedWorld,
    ) -> ChunkViewIter<
        'b,
        'c,
        V,
        <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
    >
    where
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.iter_chunks_unchecked(world) }
    }

    /// Gets an iterator which iterates through all chunks that match the query.
    pub fn iter_chunks<'b, 'c>(
        &'c mut self,
        world: &mut PreparedWorld,
    ) -> ChunkViewIter<
        'b,
        'c,
        V,
        <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
        <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
    > {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.iter_chunks_unchecked(world) }
    }

    /// Gets an iterator which iterates through all entity data that matches the query, and also yields the the `Entity` IDs.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[inline]
    pub unsafe fn iter_entities_unchecked<'b, 'c>(
        &'c mut self,
        world: &PreparedWorld,
    ) -> ChunkEntityIter<
        'b,
        V,
        ChunkViewIter<
            'b,
            'c,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    > {
        (&mut *self.query).iter_entities_unchecked(&*world.world)
    }

    /// Gets an iterator which iterates through all entity data that matches the query, and also yields the the `Entity` IDs.
    #[inline]
    pub fn iter_entities_immutable<'b, 'c>(
        &'c mut self,
        world: &PreparedWorld,
    ) -> ChunkEntityIter<
        'b,
        V,
        ChunkViewIter<
            'b,
            'c,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    >
    where
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.iter_entities_unchecked(world) }
    }

    /// Gets an iterator which iterates through all entity data that matches the query, and also yields the the `Entity` IDs.
    #[inline]
    pub fn iter_entities<'b, 'c>(
        &'c mut self,
        world: &mut PreparedWorld,
    ) -> ChunkEntityIter<
        'b,
        V,
        ChunkViewIter<
            'b,
            'c,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    > {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.iter_entities_unchecked(world) }
    }

    /// Gets an iterator which iterates through all entity data that matches the query.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[inline]
    pub unsafe fn iter_unchecked<'b, 'data>(
        &'b mut self,
        world: &PreparedWorld,
    ) -> ChunkDataIter<
        'data,
        V,
        ChunkViewIter<
            'data,
            'b,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    > {
        (&mut *self.query).iter_unchecked(&*world.world)
    }

    /// Gets an iterator which iterates through all entity data that matches the query.
    #[inline]
    pub fn iter_immutable<'b, 'data>(
        &'b mut self,
        world: &PreparedWorld,
    ) -> ChunkDataIter<
        'data,
        V,
        ChunkViewIter<
            'data,
            'b,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    >
    where
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.iter_unchecked(world) }
    }

    /// Gets an iterator which iterates through all entity data that matches the query.
    #[inline]
    pub fn iter<'b, 'data>(
        &'b mut self,
        world: &mut PreparedWorld,
    ) -> ChunkDataIter<
        'data,
        V,
        ChunkViewIter<
            'data,
            'b,
            V,
            <<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter,
            <<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter,
        >,
    > {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.iter_unchecked(world) }
    }

    /// Iterates through all entity data that matches the query.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[inline]
    pub unsafe fn for_each_unchecked<'b, 'data, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'data>>::Iter as Iterator>::Item),
    {
        (&mut *self.query).for_each_unchecked(&*world.world, f)
    }

    /// Iterates through all entity data that matches the query.
    #[inline]
    pub fn for_each_immutable<'b, 'data, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'data>>::Iter as Iterator>::Item),
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.for_each_unchecked(world, f) }
    }

    /// Iterates through all entity data that matches the query.
    #[inline]
    pub fn for_each<'b, 'data, T>(&'b mut self, world: &mut PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'data>>::Iter as Iterator>::Item),
    {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.for_each_unchecked(world, f) }
    }

    /// Iterates through all entity data that matches the query.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub unsafe fn for_each_entities_unchecked<'b, 'data, T>(
        &'b mut self,
        world: &PreparedWorld,
        f: T,
    ) where
        T: Fn((Entity, <<V as View<'data>>::Iter as Iterator>::Item)),
    {
        (&mut *self.query).for_each_entities_unchecked(&*world.world, f)
    }

    /// Iterates through all entity data that matches the query.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn for_each_entities_immutable<'b, 'data, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn((Entity, <<V as View<'data>>::Iter as Iterator>::Item)),
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.for_each_entities_unchecked(world, f) }
    }

    /// Iterates through all entity data that matches the query.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn for_each_entities<'b, 'data, T>(&'b mut self, world: &mut PreparedWorld, f: T)
    where
        T: Fn((Entity, <<V as View<'data>>::Iter as Iterator>::Item)),
    {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.for_each_entities_unchecked(world, f) }
    }

    /// Iterates through all entities that matches the query in parallel by chunk.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub unsafe fn par_entities_for_each_unchecked<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn((Entity, <<V as View<'b>>::Iter as Iterator>::Item)) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        (&mut *self.query).par_entities_for_each_unchecked(&*world.world, f)
    }

    /// Iterates through all entities that matches the query in parallel by chunk.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_entities_for_each_immutable<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn((Entity, <<V as View<'b>>::Iter as Iterator>::Item)) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.par_entities_for_each_unchecked(world, f) }
    }

    /// Iterates through all entities that matches the query in parallel by chunk.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_entities_for_each<'b, T>(&'b mut self, world: &mut PreparedWorld, f: T)
    where
        T: Fn((Entity, <<V as View<'b>>::Iter as Iterator>::Item)) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.par_entities_for_each_unchecked(world, f) }
    }

    /// Iterates through all entity data that matches the query in parallel.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub unsafe fn par_for_each_unchecked<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'b>>::Iter as Iterator>::Item) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        (&mut *self.query).par_for_each_unchecked(&*world.world, f)
    }

    /// Iterates through all entity data that matches the query in parallel.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_for_each_immutable<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'b>>::Iter as Iterator>::Item) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.par_for_each_unchecked(world, f) }
    }

    /// Iterates through all entity data that matches the query in parallel.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_for_each<'b, T>(&'b mut self, world: &mut PreparedWorld, f: T)
    where
        T: Fn(<<V as View<'b>>::Iter as Iterator>::Item) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.par_for_each_unchecked(world, f) }
    }

    /// Gets a parallel iterator of chunks that match the query.
    /// Does not perform static borrow checking.
    ///
    /// # Safety
    ///
    /// Incorrectly accessing components that are already borrowed elsewhere is undefined behavior.
    ///
    /// # Panics
    ///
    /// This function may panic if other code is concurrently accessing the same components.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub unsafe fn par_for_each_chunk_unchecked<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(Chunk<'b, V>) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        (&mut *self.query).par_for_each_chunk_unchecked(&*world.world, f)
    }

    /// Gets a parallel iterator of chunks that match the query.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_for_each_chunk_immutable<'b, T>(&'b mut self, world: &PreparedWorld, f: T)
    where
        T: Fn(Chunk<'b, V>) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
        V: ReadOnly,
    {
        // safe because the view can only read data immutably
        unsafe { self.par_for_each_chunk_unchecked(world, f) }
    }

    /// Gets a parallel iterator of chunks that match the query.
    #[cfg(feature = "par-iter")]
    #[inline]
    pub fn par_for_each_chunk<'b, T>(&'b mut self, world: &mut PreparedWorld, f: T)
    where
        T: Fn(Chunk<'b, V>) + Send + Sync,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ArchetypeFilter as Filter<
            ArchetypeFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunksetFilter as Filter<
            ChunksetFilterData<'b>,
        >>::Iter: FissileIterator,
        <<<V as DefaultFilter>::Filter as EntityFilter>::ChunkFilter as Filter<
            ChunkFilterData<'b>,
        >>::Iter: FissileIterator,
    {
        // safe because the &mut PreparedWorld ensures exclusivity
        unsafe { self.par_for_each_chunk_unchecked(world, f) }
    }
}

impl<'a, V> SystemDataOutput<'a> for PreparedQuery<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Sync,
{
    type SystemData = Query<V>;
}
