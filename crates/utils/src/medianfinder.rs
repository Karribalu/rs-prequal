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
            Some(*self.max_heap.peek().unwrap())
        } else {
            let max_top = *self.max_heap.peek().unwrap();
            let min_top = self.min_heap.peek().unwrap().0;
            Some((max_top + min_top) / 2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MedianFinder;

    #[test]
    fn test_empty_median_finder() {
        let median_finder = MedianFinder::new();
        assert_eq!(median_finder.find_median(), None);
    }

    #[test]
    fn test_single_element() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(5);
        assert_eq!(median_finder.find_median(), Some(5));
    }

    #[test]
    fn test_two_elements() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(5);
        median_finder.add_latency(10);
        assert_eq!(median_finder.find_median(), Some(7)); // (5 + 10) / 2 = 7
    }

    #[test]
    fn test_odd_number_of_elements() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(1);
        median_finder.add_latency(2);
        median_finder.add_latency(3);
        assert_eq!(median_finder.find_median(), Some(2)); // Middle element
    }

    #[test]
    fn test_even_number_of_elements() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(1);
        median_finder.add_latency(2);
        median_finder.add_latency(3);
        median_finder.add_latency(4);
        assert_eq!(median_finder.find_median(), Some(2)); // (2 + 3) / 2 = 2
    }

    #[test]
    fn test_large_numbers() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(1_000_000_000);
        median_finder.add_latency(2_000_000_000);
        median_finder.add_latency(3_000_000_000);
        assert_eq!(median_finder.find_median(), Some(2_000_000_000)); // Middle element
    }

    #[test]
    fn test_duplicate_elements() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(5);
        median_finder.add_latency(5);
        median_finder.add_latency(5);
        assert_eq!(median_finder.find_median(), Some(5)); // All elements are the same
    }


    #[test]
    fn test_large_even_number_of_elements() {
        let mut median_finder = MedianFinder::new();
        for i in 1..=100 {
            median_finder.add_latency(i);
        }
        assert_eq!(median_finder.find_median(), Some(50)); // (50 + 51) / 2 = 50
    }

    #[test]
    fn test_large_odd_number_of_elements() {
        let mut median_finder = MedianFinder::new();
        for i in 1..=101 {
            median_finder.add_latency(i);
        }
        assert_eq!(median_finder.find_median(), Some(51)); // Middle element
    }

    #[test]
    fn test_random_order() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(10);
        median_finder.add_latency(1);
        median_finder.add_latency(5);
        median_finder.add_latency(3);
        median_finder.add_latency(7);
        assert_eq!(median_finder.find_median(), Some(5)); // Sorted: [1, 3, 5, 7, 10]
    }

    #[test]
    fn test_all_elements_same() {
        let mut median_finder = MedianFinder::new();
        for _ in 0..100 {
            median_finder.add_latency(42);
        }
        assert_eq!(median_finder.find_median(), Some(42)); // All elements are 42
    }

    #[test]
    fn test_interleaved_elements() {
        let mut median_finder = MedianFinder::new();
        median_finder.add_latency(1);
        median_finder.add_latency(100);
        median_finder.add_latency(2);
        median_finder.add_latency(99);
        median_finder.add_latency(3);
        median_finder.add_latency(98);
        assert_eq!(median_finder.find_median(), Some(50)); // Sorted: [1, 2, 3, 98, 99, 100], median = (3 + 98) / 2 = 50
    }
}
