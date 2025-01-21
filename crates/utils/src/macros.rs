#[macro_export]
macro_rules! measure_time {
    ($block:block) => {{
        let start = std::time::Instant::now();
        let result = { $block }; // Execute the block and capture the result
        let duration = start.elapsed();
        println!("Execution time: {:?}", duration);
        (result, duration)
    }};
}