use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Debug, Default)]
pub struct MedianFinder {
    max_heap: BinaryHeap<u128>,           // Max-heap for the left half
    min_heap: BinaryHeap<Reverse<u128>>, // Min-heap for the right half (using Reverse)
}

impl MedianFinder {
    fn new() -> Self {
        Self {
            max_heap: BinaryHeap::new(),
            min_heap: BinaryHeap::new(),
        }
    }

    pub fn add_latency(&mut self, num: u128) {
        // Add to max-heap first
        self.max_heap.push(num);

        // Balance: move the largest from max-heap to min-heap
        if let Some(max_heap_top) = self.max_heap.pop() {
            self.min_heap.push(Reverse(max_heap_top));
        }

        // Ensure min-heap doesn't exceed max-heap in size
        if self.min_heap.len() > self.max_heap.len() {
            if let Some(Reverse(min_heap_top)) = self.min_heap.pop() {
                self.max_heap.push(min_heap_top);
            }
        }
    }

    pub fn find_median(&self) -> Option<u128> {
        if self.max_heap.is_empty() {
            return None;
        }
        if self.max_heap.len() > self.min_heap.len() {
            Some(self.max_heap.peek().unwrap().clone())
        } else {
            let max_top = self.max_heap.peek().unwrap().clone();
            let min_top = self.min_heap.peek().unwrap().0.clone();
            Some((max_top + min_top) / 2.0 as u128)
        }
    }
}