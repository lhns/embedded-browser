//! Size probe: a runtime with a full context evaluating a script read at run time.
//! Compare its stripped size with `size_without_qjs` for QuickJS-ng's code contribution.
use std::io::Read;

fn main() {
    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src).unwrap();
    let rt = rquickjs::Runtime::new().unwrap();
    let ctx = rquickjs::Context::full(&rt).unwrap();
    let out: String = ctx.with(|ctx| ctx.eval(src).unwrap_or_default());
    println!("{out}");
}
