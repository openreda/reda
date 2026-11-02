//! Data computed from a Document and kept beside it until the Document changes, such as
//! a board's copper index. Crates above the model store their own types here, keyed by
//! type and Document; any change to a Document (through a transaction, undo or redo)
//! forgets what was computed from it.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::Uuid;

type Slot = Arc<dyn Any + Send + Sync>;

/// The cache. It is not part of the Project's data: cloning a Project starts it empty and
/// comparing Projects ignores it.
#[derive(Default)]
pub(crate) struct Derived(Mutex<BTreeMap<(Uuid, TypeId), Slot>>);

impl Derived {
    /// The `T` computed for `document`, computing it with `build` when there is none.
    pub(crate) fn get_or<T: Any + Send + Sync>(
        &self,
        document: Uuid,
        build: impl FnOnce() -> T,
    ) -> Arc<T> {
        let key = (document, TypeId::of::<T>());
        if let Some(found) = self.lock().get(&key).cloned()
            && let Ok(found) = found.downcast::<T>()
        {
            return found;
        }
        // Built without the lock held, so `build` may read other derived data.
        let built = Arc::new(build());
        self.lock().insert(key, built.clone());
        built
    }

    /// Forgets everything computed from `document`.
    pub(crate) fn forget(&self, document: Uuid) {
        self.lock().retain(|(d, _), _| *d != document);
    }

    /// Forgets everything.
    pub(crate) fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(Uuid, TypeId), Slot>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Clone for Derived {
    fn clone(&self) -> Derived {
        Derived::default()
    }
}

impl PartialEq for Derived {
    fn eq(&self, _: &Derived) -> bool {
        true
    }
}

impl Eq for Derived {}

impl std::fmt::Debug for Derived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Derived")
    }
}
