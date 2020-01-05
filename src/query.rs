//! Type-level query APIs as wrappers over Legion queries.

use crate::system::SystemCtx;
use crate::{MacroData, ResourceId, Resources, SystemData, SystemDataOutput};
use hashbrown::HashSet;
use legion::borrow::{Ref, RefMut};
use legion::entity::Entity;
use legion::filter::{
    ArchetypeFilterData, ChunkFilterData, ChunksetFilterData, EntityFilter, Filter,
};
use legion::iterator::FissileIterator;
use legion::query::{
    Chunk, ChunkDataIter, ChunkEntityIter, ChunkViewIter, DefaultFilter, IntoQuery, ReadOnly, View,
};
use legion::storage::{Component, ComponentTypeId};
use legion::world::World;

/// A `legion::World` wrapper which can be safely passed to systems.
pub struct PreparedWorld {
    pub(crate) world: *const World,
    read_components: HashSet<ComponentTypeId>,
    write_components: HashSet<ComponentTypeId>,
}

assert_impl_all!(World: Send, Sync);
unsafe impl Send for PreparedWorld {}
unsafe impl Sync for PreparedWorld {}

impl<'a> SystemData<'a> for PreparedWorld {
    type Output = &'a mut Self;

    unsafe fn load_from_resources(
        _resources: &mut Resources,
        _ctx: SystemCtx,
        world: &World,
    ) -> Self {
        Self {
            world: world as *const _,
            read_components: HashSet::new(),
            write_components: HashSet::new(),
        }
    }

    fn init(
        &mut self,
        _resources: &mut Resources,
        component_reads: &[ComponentTypeId],
        component_writes: &[ComponentTypeId],
    ) {
        self.read_components.extend(component_reads);
        self.read_components.extend(component_writes);
        self.write_components.extend(component_writes);
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }
}

impl PreparedWorld {
    /// Retrieves a component immutably for the given entity.
    ///
    /// # Panics
    /// Panics if this system does not have read or write access to the component.
    /// To declare access to a component, add a query to the system which
    /// accesses the component.
    pub fn get_component<T: Component>(&self, entity: Entity) -> Option<Ref<T>> {
        assert!(self.read_components.contains(&ComponentTypeId::of::<T>()));
        unsafe { &*self.world }.get_component(entity)
    }

    /// Retrieves a component mutably for the given entity.
    ///
    /// # Panics
    /// Panics if this system does not have write access to the component.
    /// To declare access to a component, add a query to the system which
    /// accesses the component.
    pub fn get_component_mut<T: Component>(&mut self, entity: Entity) -> Option<RefMut<T>> {
        assert!(self.write_components.contains(&ComponentTypeId::of::<T>()));
        unsafe { self.get_component_mut_unchecked(entity) }
    }

    /// Retrieves a component immutably for the given entity.
    ///
    /// # Safety
    /// Accessing a component which is already being concurrently accessed elsewhere is undefined behavior.
    ///
    /// # Panics
    /// Panics if this system does not have read or write access to the component.
    /// To declare access to a component, add a query to the system which
    /// accesses the component.
    pub unsafe fn get_component_mut_unchecked<T: Component>(
        &self,
        entity: Entity,
    ) -> Option<RefMut<T>> {
        assert!(self.write_components.contains(&ComponentTypeId::of::<T>()));
        (&*self.world).get_component_mut_unchecked(entity)
    }

    /// Determines if the given `Entity` is alive within this `World`.
    pub fn is_alive(&self, entity: Entity) -> bool {
        unsafe { &*self.world }.is_alive(entity)
    }
}

impl<'a> SystemDataOutput<'a> for &'a mut PreparedWorld {
    type SystemData = PreparedWorld;
}

impl MacroData for &'static mut PreparedWorld {
    type SystemData = PreparedWorld;
}

/// System data which allows for querying entities.
pub struct Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
{
    query: legion::query::Query<V, <V as DefaultFilter>::Filter>,
}

impl<'a, V> SystemData<'a> for Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type Output = &'a mut Self;

    unsafe fn load_from_resources(
        _resources: &mut Resources,
        _ctx: SystemCtx,
        _world: &World,
    ) -> Self {
        Self { query: V::query() }
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        V::read_types()
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        V::write_types()
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }
}

impl<'a, V> SystemDataOutput<'a> for &'a mut Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type SystemData = Query<V>;
}

impl<V> MacroData for &'static mut Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync,
{
    type SystemData = Query<V>;
}

impl<V> Query<V>
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
        self.query.iter_chunks_unchecked(&*world.world)
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
        self.query.iter_entities_unchecked(&*world.world)
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
        self.query.iter_unchecked(&*world.world)
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
        self.query.for_each_unchecked(&*world.world, f)
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
    #[inline]
    pub unsafe fn for_each_entities_unchecked<'b, 'data, T>(
        &'b mut self,
        world: &PreparedWorld,
        f: T,
    ) where
        T: Fn((Entity, <<V as View<'data>>::Iter as Iterator>::Item)),
    {
        self.query.for_each_entities_unchecked(&*world.world, f)
    }

    /// Iterates through all entity data that matches the query.
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
        self.query.par_entities_for_each_unchecked(&*world.world, f)
    }

    /// Iterates through all entities that matches the query in parallel by chunk.
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
        self.query.par_for_each_unchecked(&*world.world, f)
    }

    /// Iterates through all entity data that matches the query in parallel.
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
        self.query.par_for_each_chunk_unchecked(&*world.world, f)
    }

    /// Gets a parallel iterator of chunks that match the query.
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
