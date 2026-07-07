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

    pub fn pop(&mut self) -> Option<E> {
        self.queue.pop_front()
    }
}
