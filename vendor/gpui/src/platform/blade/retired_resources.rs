/// Resources removed from the atlas remain alive until their last GPU submission completes.
pub(super) struct RetiredResources<T, S> {
    pending: Vec<(T, Option<S>)>,
}

impl<T, S> Default for RetiredResources<T, S> {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
        }
    }
}

impl<T, S> RetiredResources<T, S> {
    pub(super) fn retire(&mut self, resource: T, submission: Option<S>) {
        self.pending.push((resource, submission));
    }

    pub(super) fn release_completed(
        &mut self,
        mut completed: impl FnMut(&S) -> bool,
        mut release: impl FnMut(T),
    ) {
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].1.as_ref().is_none_or(&mut completed) {
                let (resource, _) = self.pending.swap_remove(index);
                release(resource);
            } else {
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    struct Texture {
        id: usize,
        in_flight: Rc<RefCell<Vec<usize>>>,
        destroyed: Rc<RefCell<Vec<usize>>>,
    }

    impl Drop for Texture {
        fn drop(&mut self) {
            assert!(
                !self.in_flight.borrow().contains(&self.id),
                "texture freed while GPU still uses it"
            );
            self.destroyed.borrow_mut().push(self.id);
        }
    }

    #[test]
    fn switching_previews_keeps_each_texture_alive_until_its_submission_completes() {
        let in_flight = Rc::new(RefCell::new(Vec::new()));
        let destroyed = Rc::new(RefCell::new(Vec::new()));
        let mut retired = RetiredResources::default();
        for id in 0..100 {
            in_flight.borrow_mut().push(id);
            retired.retire(
                Texture {
                    id,
                    in_flight: in_flight.clone(),
                    destroyed: destroyed.clone(),
                },
                Some(id),
            );
            retired.release_completed(|submission| !in_flight.borrow().contains(submission), drop);
            assert!(destroyed.borrow().is_empty());
        }
        // Completion can be polled repeatedly, including while a context menu animates.
        for id in (0..100).rev() {
            in_flight.borrow_mut().retain(|in_use| *in_use != id);
            retired.release_completed(|submission| !in_flight.borrow().contains(submission), drop);
            assert_eq!(destroyed.borrow().len(), 100 - id);
        }
        retired.release_completed(|_| true, drop);
        assert_eq!(destroyed.borrow().len(), 100);
    }

    #[test]
    fn unsubmitted_resources_release_without_waiting_for_a_frame() {
        let mut retired = RetiredResources::<_, usize>::default();
        retired.retire(1, None);
        retired.retire(2, Some(10));
        let mut released = Vec::new();
        retired.release_completed(|_| false, |resource| released.push(resource));
        assert_eq!(released, [1]);
        retired.release_completed(|_| true, |resource| released.push(resource));
        assert_eq!(released, [1, 2]);
    }
}
