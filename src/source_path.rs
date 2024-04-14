use std::{fmt::Display, ops::Deref, sync::Arc};

#[cfg(target_os = "windows")]
use std::path;

use allocative::Allocative;
use camino::Utf8Path;
use dupe::Dupe;
use serde::Serialize;
use starlark::{
    environment::{Methods, MethodsBuilder, MethodsStatic},
    starlark_module, starlark_simple_value,
    values::{list::AllocList, Demand, Heap, StarlarkValue, Value, ValueError},
};
use starlark_derive::{starlark_value, ProvidesStaticType};

#[derive(Clone, Debug, PartialEq, Eq, Dupe)]
pub struct SourcePath {
    pub abs_path: Arc<Utf8Path>,
    pub pretty_path: PrettyPath,
}

impl SourcePath {
    pub fn new(path: &Utf8Path, base_dir: &Utf8Path) -> Self {
        Self {
            abs_path: path.into(),
            pretty_path: PrettyPath::new(
                path.strip_prefix(base_dir).expect("path not in base dir"),
            ),
        }
    }

    pub fn new_absolute(path: &Utf8Path) -> Self {
        Self {
            abs_path: path.into(),
            pretty_path: PrettyPath::new(path),
        }
    }

    pub fn as_str(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl AsRef<str> for SourcePath {
    fn as_ref(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl Display for SourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty_path.fmt(f)
    }
}

#[derive(
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Allocative,
    Serialize,
    ProvidesStaticType,
)]
pub struct PrettyPath {
    #[allocative(skip)]
    path: Arc<Utf8Path>,

    #[cfg(target_os = "windows")]
    sanitised_path: Arc<str>,
}
starlark_simple_value!(PrettyPath);

impl PrettyPath {
    pub fn new(path: &Utf8Path) -> Self {
        Self {
            path: Arc::from(path),

            #[cfg(target_os = "windows")]
            sanitised_path: path.as_str().replace(path::MAIN_SEPARATOR, "/").into(),
        }
    }

    pub fn as_str(&self) -> &str {
        #[cfg(not(target_os = "windows"))]
        return self.path.as_str();
        #[cfg(target_os = "windows")]
        return self.sanitised_path.as_ref();
    }

    pub fn len(&self) -> usize {
        self.path.components().count()
    }

    #[starlark_module]
    fn methods(builder: &mut MethodsBuilder) {
        fn matches<'v>(this: Value<'v>, other: Value<'v>) -> starlark::Result<bool> {
            let this = this
                .request_value::<&PrettyPath>()
                .expect("receiver has incorrect type");

            if let Some(other) = other.request_value::<&PrettyPath>() {
                return Ok(this == other);
            }

            Ok(other
                .unpack_str()
                .is_some_and(|other| this.as_str() == other))
        }
    }
}

impl From<&str> for PrettyPath {
    fn from(value: &str) -> Self {
        Self::new(Utf8Path::new(value))
    }
}

impl AsRef<Utf8Path> for PrettyPath {
    fn as_ref(&self) -> &Utf8Path {
        &self.path
    }
}

impl Deref for PrettyPath {
    type Target = Utf8Path;

    fn deref(&self) -> &Self::Target {
        self.path.as_ref()
    }
}

impl Display for PrettyPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(not(target_os = "windows"))]
        return self.path.fmt(f);
        #[cfg(target_os = "windows")]
        return self.sanitised_path.fmt(f);
    }
}

#[starlark_value(type = "Path")]
impl<'v> StarlarkValue<'v> for PrettyPath {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(Self::methods)
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        Ok(other
            .request_value::<&Self>()
            .map(|other| self == other)
            .unwrap_or_default())
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(self.len() as i32)
    }

    fn is_in(&self, other: Value<'v>) -> starlark::Result<bool> {
        let Some(str) = other.unpack_str() else {
            return Err(ValueError::IncorrectParameterTypeWithExpected(
                "str".to_owned(),
                other.get_type().to_owned(),
            )
            .into());
        };
        Ok(self.components().any(|c| c.as_str() == str))
    }

    fn iterate_collect(&self, heap: &'v Heap) -> starlark::Result<Vec<Value<'v>>> {
        Ok(self.components().map(|c| heap.alloc(c.as_str())).collect())
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        let Some(mut index) = index.unpack_i32() else {
            return ValueError::unsupported_with(self, "[]", index)?;
        };
        let n = self.len() as i32;
        if index >= n || index < -n {
            return Err(ValueError::IndexOutOfBound(index).into());
        }
        if index < 0 {
            index += n;
        }
        let component = {
            if index >= 0 {
                self.components().nth(index as usize)
            } else {
                self.components().nth_back((-1 - index) as usize)
            }
            .expect("index computation incorrect")
        };
        Ok(heap.alloc(component.as_str()))
    }

    fn slice(
        &self,
        start: Option<Value<'v>>,
        stop: Option<Value<'v>>,
        stride: Option<Value<'v>>,
        heap: &'v Heap,
    ) -> starlark::Result<Value<'v>> {
        let n = self.len() as i32;
        let start = start.and_then(Value::unpack_i32);
        let stop = stop.and_then(Value::unpack_i32);
        let stride = stride.and_then(Value::unpack_i32).unwrap_or(1);
        if stride == 0 {
            return Err(ValueError::IndexOutOfBound(stride).into());
        }

        if stride > 0 {
            let normalise_index = |idx: i32| if idx < 0 { idx + n } else { idx }.clamp(0, n);
            let low = start.map(normalise_index).unwrap_or(0);
            let high = stop.map(normalise_index).unwrap_or(n);
            if high <= low {
                // Empty result fast path.
                return Ok(heap.alloc(AllocList::<[i32; 0]>([])));
            }
            Ok(heap.alloc(AllocList(
                self.components()
                    .enumerate()
                    .map(|(i, c)| (i as i32, c))
                    .skip_while(|(i, _)| *i < low)
                    .take_while(|(i, _)| *i < high)
                    .filter(|(i, _)| (i - low) % stride == 0)
                    .map(|(_, c)| c.as_str()),
            )))
        } else {
            let normalise_index = |idx: i32| if idx < 0 { idx + n } else { idx }.clamp(-1, n - 1);
            let high = start.map(normalise_index).unwrap_or(n - 1);
            let low = stop.map(normalise_index).unwrap_or(-1);
            if high <= low {
                // Empty result fast path.
                return Ok(heap.alloc(AllocList::<[i32; 0]>([])));
            }
            Ok(heap.alloc(AllocList(
                self.components()
                    .rev()
                    .enumerate()
                    .map(|(i, c)| (n - i as i32 - 1, c))
                    .skip_while(|(i, _)| *i > high)
                    .take_while(|(i, _)| *i > low)
                    .filter(|(i, _)| (*i - high) % -stride == 0)
                    .map(|(_, c)| c.as_str()),
            )))
        }
    }
}

#[cfg(test)]
mod test {
    use indoc::{formatdoc, indoc};
    use lazy_static::lazy_static;
    use regex::Regex;
    use starlark::{
        environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module},
        eval::{Evaluator, FileLoader},
        syntax::{AstModule, Dialect},
    };

    use crate::{error::Error, scriptlets::PrintHandler, vextest::VexTest};

    use super::*;

    struct PathTest<'s> {
        name: &'s str,
        path: Option<&'s str>,
    }

    impl<'s> PathTest<'s> {
        fn new(name: &'s str) -> Self {
            Self { name, path: None }
        }

        fn path(mut self, path: &'s str) -> Self {
            self.path = Some(path);
            self
        }

        fn run(self, to_run: impl AsRef<str>) {
            self.try_run(to_run).unwrap()
        }

        fn try_run(self, to_run: impl AsRef<str>) -> starlark::Result<()> {
            self.setup();

            let path = PrettyPath::new(Utf8Path::new(self.path.expect("path not set")));
            let module = Module::new();
            module.set("path", module.heap().alloc(path));

            let code = formatdoc! {r#"
                    load('{check_path}', 'check')
                    {to_run}
                "#,
                check_path = VexTest::CHECK_STARLARK_PATH,
                to_run = to_run.as_ref(),
            };
            let dialect = Dialect {
                enable_top_level_stmt: true,
                ..Dialect::Standard
            };
            let ast = AstModule::parse("vexes/test.star", code.to_string(), &dialect).unwrap();
            let mut eval = Evaluator::new(&module);
            eval.set_print_handler(&PrintHandler);
            eval.set_loader(&TestModuleCache);
            eval.eval_module(ast, &Self::globals()).map(|_| ())
        }

        fn globals() -> Globals {
            GlobalsBuilder::extended_by(&[LibraryExtension::Print]).build()
        }

        fn setup(&self) {
            eprintln!("running test {}...", self.name);
        }
    }

    struct TestModuleCache;

    impl FileLoader for TestModuleCache {
        fn load(&self, path: &str) -> anyhow::Result<starlark::environment::FrozenModule> {
            if path != VexTest::CHECK_STARLARK_PATH {
                let path = PrettyPath::from(path);
                return Err(Error::NoSuchModule(path).into());
            }
            lazy_static! {
                static ref CHECK_MODULE: FrozenModule = {
                    let module = Module::new();
                    {
                        let mut eval = Evaluator::new(&module);
                        let ast = AstModule::parse(
                            VexTest::CHECK_STARLARK_PATH,
                            VexTest::CHECK_SRC.to_string(),
                            &Dialect::Standard,
                        )
                        .unwrap();
                        eval.eval_module(ast, &Globals::standard()).unwrap();
                    }
                    module.freeze().unwrap()
                };
            }
            Ok(CHECK_MODULE.deref().dupe())
        }
    }

    #[test]
    fn equals() {
        let path = "src/main.rs";
        PathTest::new("equals").path(path).run(formatdoc! {r#"
            check['eq'](path, path)
            check['neq'](path, '{path}')
            check['neq']('{path}', path)
        "#});
    }

    #[test]
    fn matches() {
        PathTest::new("matches").path("src/main.rs").run(indoc! {r#"
            check['true'](path.matches("src/main.rs"))
            check['false'](path.matches(None))
            check['false'](path.matches(''))
            check['false'](path.matches("src/lib.rs"))
        "#});
    }

    #[test]
    fn len() {
        PathTest::new("absolute-unix")
            .path("/")
            .run("check['eq'](len(path), 1)");
        PathTest::new("absolute-windows")
            .path("A:")
            .run("check['eq'](len(path), 1)");
        PathTest::new("normal-unix")
            .path("src/main.rs")
            .run("check['eq'](len(path), 2)");
    }

    #[test]
    fn r#in() {
        PathTest::new("in").path("src/main.rs").run(indoc! {r#"
            check['in']('src', path)
            check['in']('main.rs', path)
        "#})
    }

    #[test]
    fn iterate() {
        PathTest::new("iter").path("src/main.rs").run(indoc! {r#"
            for (part, expected) in zip(path, ['src', 'main.rs']):
                check['eq'](part, expected)
        "#})
    }

    #[test]
    fn at() {
        {
            let expected = "Index `0` is out of bound";
            let err = PathTest::new("empty")
                .path("")
                .try_run("path[0]")
                .unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "unexpected error: expected {expected:?} but got {err}"
            )
        }

        {
            const PATH: &str = "src/foo/bar/baz/main.rs";
            let n = 1 + PATH.chars().filter(|c| *c == '/').count() as i64;
            for index in (-2 * n..-n).chain(n..2 * n) {
                let expected = Regex::new("Index `-?[0-9]+` is out of bound").unwrap();
                let err = PathTest::new("out-of-bounds")
                    .path(PATH)
                    .try_run(format!("path[{index}]"))
                    .unwrap_err();
                assert!(
                    expected.is_match(&err.to_string()),
                    "unexpected error: expected {expected:?} but got {err}"
                )
            }
        }

        PathTest::new("populated")
            .path("src/foo/bar/baz/main.rs")
            .run(indoc! {r#"
                expected = ['src', 'foo', 'bar', 'baz', 'main.rs']
                n = len(expected)
                for i in range(-n, n - 1):
                    check['eq'](path[i], expected[i])
            "#})
    }

    #[test]
    fn slice() {
        PathTest::new("ok-indices")
            .path("src/foo/bar/baz/main.rs")
            .run(indoc! {r#"
                expected = ['src', 'foo', 'bar', 'baz', 'main.rs']

                def gen_test_indices(expected, min=2*len(expected), max=2*len(expected)):
                    ret = [None]
                    ret.extend(range(min, max+1))
                    return ret
                starts = gen_test_indices(expected)
                stops = gen_test_indices(expected)
                strides = gen_test_indices(expected)
                tests = [(start, stop, stride) for start in starts for stop in stops for stride in strides]

                errs = []
                def eq(start, stop, stride, a, b):
                    if a != b:
                        errs.append(('[%r:%r:%r]' % (start, stop, stride), a, b))
                        print('%r, %r, %r' % (start, stop, stride), a, b)
                def test(start, stop, stride):
                    if stride == 0:
                        return

                    if start != None:
                        if stop != None:
                            if stride != None:
                                eq(start, stop, stride, path[start:stop:stride], expected[start:stop:stride])
                            else:
                                eq(start, stop, stride, path[start:stop:], expected[start:stop:])
                        else:
                            if stride != None:
                                eq(start, stop, stride, path[start::stride], expected[start::stride])
                            else:
                                eq(start, stop, stride, path[start::], expected[start::])
                    else:
                        if stop != None:
                            if stride != None:
                                eq(start, stop, stride, path[:stop:stride], expected[:stop:stride])
                            else:
                                eq(start, stop, stride, path[:stop:], expected[:stop:])
                        else:
                            if stride != None:
                                eq(start, stop, stride, path[::stride], expected[::stride])
                            else:
                                eq(start, stop, stride, path[::], expected[::])

                for (start, stop, stride) in tests:
                    test(start, stop, stride)
                for err in errs:
                    print(*err)
                if len(errs):
                    fail('encountered %d problems' % len(errs))
            "#});
        {
            let expected = "Index `0` is out of bound";
            let err = PathTest::new("zero-stride")
                .path("src/foo/bar/baz/main.rs")
                .try_run("path[::0]")
                .unwrap_err();
            assert!(
                err.to_string().contains(expected),
                "unexpected error: expected {expected:?} but got {err}"
            );
        }
    }
}
