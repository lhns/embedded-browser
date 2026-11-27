//! Size probe baseline: the same program shape without QuickJS-ng.
use std::io::Read;

fn main() {
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap();
    println!("{}", src.trim());
}
