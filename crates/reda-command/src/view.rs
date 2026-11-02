//! GUI state that view Commands change, kept in the session so that every way of running a
//! Command (menus, keys, scripts, a bridged CLI or MCP client) reaches it.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt;

/// The front end's view state, one value per type. The engine knows nothing of what is in
/// it: the app installs its own types (such as its docking Layout) and the view Commands it
/// registers read them back with [`crate::Context::view`]. Headless sessions keep it empty.
#[derive(Default)]
pub struct ViewState {
    values: BTreeMap<TypeId, Box<dyn Any + Send>>,
}

impl ViewState {
    /// Installs `value`, replacing any earlier value of its type.
    pub fn set<T: Any + Send>(&mut self, value: T) {
        self.values.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: Any + Send>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.values.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }
}

impl fmt::Debug for ViewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ViewState({} values)", self.values.len())
    }
}
