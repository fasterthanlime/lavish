use crate::codegen::rust::prelude::*;
use std::collections::HashSet;

pub trait WriteTo: Display {
    fn write_to(&self, s: &mut Scope) {
        write!(s, "{}", self).unwrap();
    }
}

impl<T> WriteTo for T where T: Display {}

pub struct Allow {
    items: Vec<&'static str>,
}

impl Allow {
    pub fn non_camel_case(mut self) -> Self {
        self.items.push("non_camel_case_types");
        self
    }

    pub fn unused(mut self) -> Self {
        self.items.push("unused");
        self
    }
}

impl Display for Allow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "#[allow({items})]", items = self.items.join(", "))
    }
}

pub fn allow() -> Allow {
    Allow { items: Vec::new() }
}

pub struct _Fn<'a> {
    kw_pub: bool,
    self_param: Option<String>,
    params: Vec<String>,
    type_params: Vec<TypeParam>,
    name: String,
    ret: Option<String>,
    body: Option<Box<Fn(&mut Scope) + 'a>>,
    self_bound: Option<String>,
}

impl<'a> _Fn<'a> {
    pub fn kw_pub(mut self) -> Self {
        self.kw_pub = true;
        self
    }

    pub fn returns<D>(mut self, ret: D) -> Self
    where
        D: Display,
    {
        self.ret = Some(format!("{}", ret));
        self
    }

    pub fn body<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut Scope) + 'a,
    {
        self.body = Some(Box::new(f));
        self
    }

    pub fn self_param<D>(mut self, self_param: D) -> Self
    where
        D: Display,
    {
        self.self_param = Some(format!("{}", self_param));
        self
    }

    pub fn type_param<N>(mut self, name: N) -> Self
    where
        N: Into<String>,
    {
        self.type_params.push(TypeParam {
            name: name.into(),
            bound: None,
        });
        self
    }

    pub fn type_param_bound<N, B>(mut self, name: N, bound: B) -> Self
    where
        N: Into<String>,
        B: Into<String>,
    {
        self.type_params.push(TypeParam {
            name: name.into(),
            bound: Some(bound.into()),
        });
        self
    }

    pub fn param<N>(mut self, name: N) -> Self
    where
        N: Into<String>,
    {
        self.params.push(name.into());
        self
    }

    pub fn self_bound<B>(mut self, bound: B) -> Self
    where
        B: Into<String>,
    {
        self.self_bound = Some(bound.into());
        self
    }
}

impl<'a> Display for _Fn<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            if self.kw_pub {
                s.write("pub ");
            }

            s.write("fn ").write(&self.name);
            s.in_list(Brackets::Angle, |l| {
                l.omit_empty();
                for tp in &self.type_params {
                    l.item(&tp.name);
                }
            });

            s.in_list(Brackets::Round, |l| {
                if let Some(self_param) = self.self_param.as_ref() {
                    l.item(self_param);
                }
                for p in &self.params {
                    l.item(&p);
                }
            });

            if let Some(ret) = self.ret.as_ref() {
                s.write(" -> ").write(ret);
            }

            if self.self_bound.is_some() || self.type_params.iter().any(|tp| tp.bound.is_some()) {
                s.lf();
                s.write("where").lf();
                s.in_scope(|s| {
                    if let Some(bound) = self.self_bound.as_ref() {
                        writeln!(s, "Self: {bound},", bound = bound).unwrap();
                    }
                    for tp in &self.type_params {
                        if let Some(bound) = tp.bound.as_ref() {
                            writeln!(s, "{name}: {bound},", name = tp.name, bound = bound).unwrap();
                        }
                    }
                });
            }

            if let Some(body) = self.body.as_ref() {
                s.in_block(|s| {
                    body(s);
                });
            } else {
                s.write(";").lf();
            }
        })
    }
}

pub fn _fn<'a, N>(name: N) -> _Fn<'a>
where
    N: Into<String>,
{
    _Fn {
        kw_pub: false,
        name: name.into(),
        params: Vec::new(),
        type_params: Vec::new(),
        self_param: None,
        body: None,
        ret: None,
        self_bound: None,
    }
}

pub struct _Impl<'a> {
    trt: Option<String>,
    name: String,
    type_params: Vec<TypeParam>,
    body: Option<Box<Fn(&mut Scope) + 'a>>,
}

impl<'a> _Impl<'a> {
    pub fn type_param<N>(mut self, name: N) -> Self
    where
        N: Into<String>,
    {
        self.type_params.push(TypeParam {
            name: name.into(),
            bound: None,
        });
        self
    }

    pub fn type_param_bound<N, B>(mut self, name: N, bound: B) -> Self
    where
        N: Into<String>,
        B: Into<String>,
    {
        self.type_params.push(TypeParam {
            name: name.into(),
            bound: Some(bound.into()),
        });
        self
    }

    pub fn body<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut Scope) + 'a,
    {
        self.body = Some(Box::new(f));
        self
    }
}

pub fn _impl_trait<'a, T, N>(trt: T, name: N) -> _Impl<'a>
where
    T: Into<String>,
    N: Into<String>,
{
    _Impl {
        trt: Some(trt.into()),
        name: name.into(),
        type_params: Vec::new(),
        body: None,
    }
}

pub fn _impl<'a, N>(name: N) -> _Impl<'a>
where
    N: Into<String>,
{
    _Impl {
        trt: None,
        name: name.into(),
        type_params: Vec::new(),
        body: None,
    }
}

impl<'a> Display for _Impl<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            s.write("impl");
            s.in_list(Brackets::Angle, |l| {
                l.omit_empty();
                for tp in &self.type_params {
                    l.item(&tp.name);
                }
            });
            if let Some(trt) = self.trt.as_ref() {
                write!(s, " {trt} for", trt = trt).unwrap();
            }
            write!(s, " {name}", name = &self.name).unwrap();
            s.in_list(Brackets::Angle, |l| {
                l.omit_empty();
                for tp in &self.type_params {
                    l.item(&tp.name);
                }
            });

            if self.type_params.iter().any(|tp| tp.bound.is_some()) {
                s.lf();
                s.write("where").lf();
                s.in_scope(|s| {
                    for tp in &self.type_params {
                        if let Some(bound) = tp.bound.as_ref() {
                            writeln!(s, "{name}: {bound},", name = tp.name, bound = bound).unwrap();
                        }
                    }
                });
            }

            s.in_block(|s| {
                if let Some(body) = self.body.as_ref() {
                    body(s);
                }
            });
        })
    }
}

#[derive(Clone)]
pub struct TypeParam {
    name: String,
    bound: Option<String>,
}

pub fn quoted<D>(d: D) -> String
where
    D: fmt::Debug,
{
    format!("{:?}", d)
}

pub struct _Enum {
    kw_pub: bool,
    name: String,
    variants: Vec<String>,
}

impl _Enum {
    pub fn kw_pub(&mut self) -> &mut Self {
        self.kw_pub = true;
        self
    }

    pub fn variant<D>(&mut self, d: D) -> &mut Self
    where
        D: Display,
    {
        self.variants.push(format!("{}", d));
        self
    }
}

pub fn _enum<S>(name: S) -> _Enum
where
    S: Into<String>,
{
    _Enum {
        name: name.into(),
        kw_pub: false,
        variants: Vec::new(),
    }
}

impl Display for _Enum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Scope::fmt(f, |s| {
            if self.kw_pub {
                s.write("pub ");
            }
            s.write("enum ").write(&self.name);
            if self.variants.is_empty() {
                s.write(" {}").lf();
            } else {
                s.in_block(|s| {
                    for variant in &self.variants {
                        s.write(variant).write(",").lf();
                    }
                });
            }
        })
    }
}

pub struct Derive {
    items: HashSet<String>,
}

impl Derive {
    pub fn debug(mut self) -> Self {
        self.items.insert("Debug".into());
        self
    }

    pub fn clone(mut self) -> Self {
        self.items.insert("Clone".into());
        self
    }

    pub fn copy(mut self) -> Self {
        self.items.insert("Copy".into());
        self
    }
}

impl Display for Derive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut items: Vec<String> = self.items.iter().cloned().collect();
        items.sort();
        writeln!(f, "#[derive({items})]", items = items.join(", "))
    }
}

pub fn derive() -> Derive {
    Derive {
        items: HashSet::new(),
    }
}
