use std::collections::VecDeque;

pub struct Events<E> {
    queue: VecDeque<E>,
}

impl<E> Default for Events<E> {
    fn default() -> Self {
        Self {
            queue: Default::default(),
        }
    }
}

impl<E> Events<E> {
    pub fn push(&mut self, event: E) {
        self.queue.push_back(event);
    }

    pub fn pop<T>(&mut self, f: impl Fn(&E) -> Option<T>) -> Option<T> {
        let mut latest = None;
        let mut remaining = VecDeque::with_capacity(self.queue.len());
        for event in self.queue.drain(..) {
            match f(&event) {
                Some(value) => latest = Some(value),
                None => remaining.push_back(event),
            }
        }
        self.queue = remaining;
        latest
    }
}
