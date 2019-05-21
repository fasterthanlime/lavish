use super::super::ast;
use super::Error;
use heck::{CamelCase, MixedCase, SnakeCase};
use indexmap::IndexMap;
use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

const INDENT_WIDTH: usize = 4;

struct Output {
    writer: RefCell<BufWriter<File>>,
}

impl Output {
    fn new(file: File) -> Self {
        Self {
            writer: RefCell::new(BufWriter::new(file)),
        }
    }

    fn write_indented(&self, indent: usize, line: &str) {
        let mut w = self.writer.borrow_mut();
        for _ in 0..indent {
            write!(w, " ").unwrap();
        }
        writeln!(w, "{}", line).unwrap();
    }
}

struct Context<'a> {
    namespaces: IndexMap<&'a str, Namespace<'a>>,
    output: Output,
}

struct Scope<'a> {
    output: &'a Output,
    indent: usize,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum FunKind {
    Request,
    Notification,
}

impl<'a> Context<'a> {
    fn new(file: File) -> Self {
        Self {
            namespaces: IndexMap::new(),
            output: Output::new(file),
        }
    }

    fn visit_toplevel_ns(&mut self, decl: &'a ast::NamespaceDecl) {
        let k = decl.name.text.as_ref();

        let mut ns = Namespace::new(decl);
        self.visit_ns("", &mut ns);

        if let Some(original_ns) = self.namespaces.get_mut(k) {
            original_ns.merge(ns)
        } else {
            self.namespaces.insert(k, ns);
        }
    }

    fn visit_ns(&mut self, prefix: &str, ns: &mut Namespace<'a>) {
        let decl = &ns.decl;
        let prefix = format!("{}{}.", prefix, ns.name());

        for decl in &decl.functions {
            let full_name = format!("{}{}", prefix, decl.name.text);
            let ff = Fun::new(decl, full_name);
            ns.funs.insert(&decl.name.text, ff);
        }

        for decl in &decl.namespaces {
            let mut child = Namespace::new(decl);
            self.visit_ns(&prefix, &mut child);
            ns.children.insert(decl.name.text.as_ref(), child);
        }
    }

    fn all_funs(&self) -> Box<Iterator<Item = &'a Fun> + 'a> {
        Box::new(self.namespaces.values().map(Namespace::funs).flatten())
    }

    fn funs(&self, kind: FunKind) -> Box<Iterator<Item = &'a Fun> + 'a> {
        let is_notification = kind == FunKind::Notification;

        Box::new(
            self.all_funs()
                .filter(move |x| x.is_notification() == is_notification),
        )
    }
}

trait ScopeLike<'a> {
    fn line(&self, line: &str);
    fn scope(&self) -> Scope;

    fn comment(&self, comment: &Option<ast::Comment>) {
        if let Some(comment) = comment.as_ref() {
            for line in &comment.lines {
                self.line(&format!("// {}", line))
            }
        }
    }

    fn def_struct(&self, name: &str, f: &Fn(&ScopeLike)) {
        self.line("#[derive(Serialize, Deserialize, Debug)]");
        self.line(&format!("pub struct {} {{", name));
        self.in_scope(f);
        self.line("}");
    }

    fn in_scope(&self, f: &Fn(&ScopeLike)) {
        let s = self.scope();
        f(&s);
    }
}

impl<'a> ScopeLike<'a> for Context<'a> {
    fn line(&self, line: &str) {
        self.output.write_indented(0, line)
    }

    fn scope(&self) -> Scope {
        Scope {
            output: &self.output,
            indent: INDENT_WIDTH,
        }
    }
}

impl<'a> ScopeLike<'a> for Scope<'a> {
    fn line(&self, line: &str) {
        self.output.write_indented(self.indent, line)
    }

    fn scope(&self) -> Scope {
        Scope {
            output: &self.output,
            indent: self.indent + INDENT_WIDTH,
        }
    }
}

struct Namespace<'a> {
    decl: &'a ast::NamespaceDecl,
    children: IndexMap<&'a str, Namespace<'a>>,

    funs: IndexMap<&'a str, Fun<'a>>,
}

impl<'a> Namespace<'a> {
    fn new(decl: &'a ast::NamespaceDecl) -> Self {
        Namespace {
            decl,
            children: IndexMap::new(),
            funs: IndexMap::new(),
        }
    }

    fn funs(&self) -> Box<Iterator<Item = &'a Fun> + 'a> {
        Box::new(
            self.children
                .values()
                .map(Namespace::funs)
                .flatten()
                .chain(self.funs.values()),
        )
    }

    fn merge(&mut self, rhs: Self) {
        for (k, v) in rhs.children {
            if let Some(sv) = self.children.get_mut(k) {
                sv.merge(v)
            } else {
                self.children.insert(k, v);
            }
        }

        for (k, v) in rhs.funs {
            self.funs.insert(k, v);
        }
    }

    fn name(&self) -> &'a str {
        &self.decl.name.text
    }
}

struct Fun<'a> {
    decl: &'a ast::FunctionDecl,
    tokens: Vec<String>,
}

impl<'a> Fun<'a> {
    fn new(decl: &'a ast::FunctionDecl, full_name: String) -> Self {
        Self {
            decl,
            tokens: full_name.split('.').map(|x| x.into()).collect(),
        }
    }

    fn rpc_name(&self) -> String {
        let last = self.tokens.len() - 1;
        self.tokens
            .iter()
            .enumerate()
            .map(|(i, x)| {
                if i == last {
                    x.to_camel_case()
                } else {
                    x.to_mixed_case()
                }
            })
            .collect::<Vec<_>>()
            .join(".")
    }

    fn variant_name(&self) -> String {
        self.rpc_name().replace(".", "_").to_lowercase()
    }

    fn qualified_name(&self) -> String {
        self.tokens.join("::")
    }

    fn mod_name(&self) -> String {
        self.decl.name.text.to_snake_case()
    }

    fn is_notification(&self) -> bool {
        self.decl
            .modifiers
            .contains(&ast::FunctionModifier::Notification)
    }
}

type Result = std::result::Result<(), Error>;

pub fn codegen<'a>(modules: &'a [ast::Module], output: &str) -> Result {
    let start_instant = Instant::now();

    let output_path = Path::new(output);
    std::fs::create_dir_all(output_path.parent().unwrap())?;
    let out = File::create(output_path).unwrap();

    let mut root = Context::new(out);

    for module in modules {
        for decl in &module.namespaces {
            root.visit_toplevel_ns(decl);
        }
    }

    let s = &root;
    s.line("// This file is generated by lavish: DO NOT EDIT");
    s.line("// https://github.com/fasterthanlime/lavish");
    s.line("");
    s.line("// Kindly ask rustfmt not to reformat this file.");
    s.line("#![cfg_attr(rustfmt, rustfmt_skip)]");
    s.line("");
    s.line("// Disable some lints, since this file is generated.");
    s.line("#![allow(clippy::all)]");
    s.line("#![allow(unknown_lints)]");
    s.line("#![allow(unused)]");
    s.line("");

    fn write_enum<'a, I>(s: &ScopeLike, kind: &str, funs: I)
    where
        I: Iterator<Item = &'a Fun<'a>>,
    {
        let s = s.scope();
        for fun in funs {
            s.line(&format!(
                "{}({}::{}),",
                fun.variant_name(),
                fun.qualified_name(),
                kind,
            ));
        }
    };

    {
        s.line("pub use __::*;");
        s.line("");
        s.line("mod __ {");
        let s = s.scope();

        s.line("// Notes: as of 2019-05-21, futures-preview is required");
        s.line("use futures::prelude::*;");
        s.line("use std::pin::Pin;");
        s.line("use std::sync::Arc;");
        s.line("");
        s.line("use lavish_rpc as rpc;");
        s.line("use lavish_rpc::serde_derive::*;");
        s.line("use lavish_rpc::erased_serde;");

        s.line("");
        s.line("#[derive(Serialize, Debug)]");
        s.line("#[serde(untagged)]");
        s.line("#[allow(non_camel_case_types, unused)]");
        s.line("pub enum Params {");
        write_enum(&s, "Params", root.funs(FunKind::Request));
        s.line("}"); // enum Params

        s.line("");
        s.line("#[derive(Serialize, Debug)]");
        s.line("#[serde(untagged)]");
        s.line("#[allow(non_camel_case_types, unused)]");
        s.line("pub enum Results {");
        write_enum(&s, "Results", root.funs(FunKind::Request));
        s.line("}"); // enum Results

        s.line("");
        s.line("#[derive(Serialize, Debug)]");
        s.line("#[serde(untagged)]");
        s.line("#[allow(non_camel_case_types, unused)]");
        s.line("pub enum NotificationParams {");
        write_enum(&s, "Params", root.funs(FunKind::Notification));
        s.line("}"); // enum NotificationParams

        s.line("");
        s.line("pub type Message = rpc::Message<Params, NotificationParams, Results>;");
        s.line("pub type Handle = rpc::Handle<Params, NotificationParams, Results>;");
        s.line("pub type System = rpc::System<Params, NotificationParams, Results>;");
        s.line("pub type Protocol = rpc::Protocol<Params, NotificationParams, Results>;");
        s.line("pub type HandlerRet = Pin<Box<dyn Future<Output = Result<Results, rpc::Error>> + Send + 'static>>;");
        s.line("");
        s.line("pub fn protocol() -> Protocol {");
        s.in_scope(&|s| {
            s.line("Protocol::new()");
        });
        s.line("}"); // fn protocol

        for (strukt, side, kind) in &[
            ("Params", "Params", FunKind::Request),
            ("Results", "Results", FunKind::Request),
            ("Params", "NotificationParams", FunKind::Notification),
        ] {
            s.line("");
            s.line(&format!("impl rpc::Atom for {} {{", side));
            s.in_scope(&|s| {
                s.line("fn method(&self) -> &'static str {");
                s.in_scope(&|s| {
                    s.line("match self {");
                    s.in_scope(&|s| {
                        for fun in root.funs(*kind) {
                            s.line(&format!(
                                "{}::{}(_) => {:?},",
                                side,
                                fun.variant_name(),
                                fun.rpc_name()
                            ));
                        }
                    });
                    s.line("}");
                });
                s.line("}"); // fn method

                s.line("");
                s.line("fn deserialize(");
                s.in_scope(&|s| {
                    s.line("method: &str,");
                    s.line("de: &mut erased_serde::Deserializer,");
                });
                s.line(") -> erased_serde::Result<Self> {");
                s.in_scope(&|s| {
                    s.line("use erased_serde::deserialize as deser;");
                    s.line("use serde::de::Error;");
                    s.line("");
                    s.line("match method {");
                    s.in_scope(&|s| {
                        for fun in root.funs(*kind) {
                            s.line(&format!("{:?} =>", fun.rpc_name(),));
                            {
                                let s = s.scope();
                                s.line(&format!(
                                    "Ok({}::{}(deser::<{}::{}>(de)?)),",
                                    side,
                                    fun.variant_name(),
                                    fun.qualified_name(),
                                    strukt,
                                ));
                            }
                        }
                        s.line("_ => Err(erased_serde::Error::custom(format!(");
                        s.in_scope(&|s| {
                            s.line(&format!("{:?},", "unknown method: {}"));
                            s.line("method,");
                        });
                        s.line("))),");
                    });
                    s.line("}");
                });
                s.line("}"); // fn deserialize
            });
            s.line("}"); // impl Atom for side
        } // impl rpc::Atom for P, NP, R

        s.line("");
        s.line("pub struct Call<T, PP> {");
        s.in_scope(&|s| {
            s.line("pub state: Arc<T>,");
            s.line("pub handle: Handle,");
            s.line("pub params: PP,");
        });
        s.line("}"); // struct Call

        s.line("");
        s.line("pub type SlotFuture = ");
        s.in_scope(&|s| {
            s.line("Future<Output = Result<Results, rpc::Error>> + Send + 'static;");
        });

        s.line("");
        s.line("pub type SlotReturn = Pin<Box<SlotFuture>>;");

        s.line("");
        s.line("pub type SlotFn<'a, T> = ");
        s.in_scope(&|s| {
            s.line("Fn(Arc<T>, Handle, Params) -> SlotReturn + 'a + Send + Sync;");
        });

        s.line("");
        s.line("pub type Slot<'a, T> = Option<Box<SlotFn<'a, T>>>;");

        s.line("");
        s.line("pub struct Handler<'a, T> {");
        s.in_scope(&|s| {
            s.line("state: Arc<T>,");
            for fun in root.funs(FunKind::Request) {
                s.line(&format!("{}: Slot<'a, T>,", fun.variant_name()));
            }
        });
        s.line("}"); // struct Handler

        for (_, ns) in &root.namespaces {
            s.line("");
            visit_ns(&s, ns, 1)?;
        }
    }
    s.line("}"); // mod __root

    let end_instant = Instant::now();
    println!(
        "Generated {:?} in {:?}",
        output_path,
        end_instant.duration_since(start_instant)
    );

    Ok(())
}

fn visit_ns<'a>(s: &'a ScopeLike<'a>, ns: &Namespace, depth: usize) -> Result {
    s.line(&format!("pub mod {} {{", ns.name()));
    {
        let s = s.scope();
        for (_, ns) in &ns.children {
            visit_ns(&s, ns, depth + 1)?;
        }

        for (_, fun) in &ns.funs {
            s.comment(&fun.decl.comment);
            s.line(&format!("pub mod {} {{", fun.mod_name()));

            {
                let s = s.scope();
                s.line("use futures::prelude::*;");
                s.line("use lavish_rpc::serde_derive::*;");
                let super_ref = "super::".repeat(depth + 2);
                s.line(&format!("use {}__;", super_ref));
                s.line("");

                let write_downgrade = |side: &str| {
                    s.in_scope(&|s| {
                        s.line(&format!(
                            "pub fn downgrade(p: __::{}) -> Option<Self> {{",
                            side,
                        ));
                        s.in_scope(&|s| {
                            s.line("match p {");
                            s.in_scope(&|s| {
                                s.line(&format!(
                                    "__::{}::{}(p) => Some(p),",
                                    side,
                                    fun.variant_name()
                                ));
                                s.line("_ => None,");
                            });
                            s.line("}"); // match p
                        });
                        s.line("}"); // fn downgrade
                    });
                };

                s.def_struct("Params", &|s| {
                    for f in &fun.decl.params {
                        s.line(&format!("pub {}: {},", f.name.text, f.typ));
                    }
                });

                s.line("");
                s.line("impl Params {");
                write_downgrade(if fun.is_notification() {
                    "NotificationParams"
                } else {
                    "Params"
                });
                s.line("}"); // impl Params

                if !fun.is_notification() {
                    s.line("");
                    s.def_struct("Results", &|s| {
                        for f in &fun.decl.results {
                            s.line(&format!("pub {}: {},", f.name.text, f.typ));
                        }
                    });

                    s.line("");
                    s.line("impl Results {");
                    write_downgrade("Results");
                    s.line("}"); // impl Results

                    s.line("");
                    s.line("pub async fn call(h: &__::Handle, p: Params) -> Result<Results, lavish_rpc::Error> {");
                    s.in_scope(&|s| {
                        s.line("h.call(");
                        s.in_scope(&|s| {
                            s.line(&format!("__::Params::{}(p),", fun.variant_name()));
                            s.line("Results::downgrade,");
                        }); // h.call arguments
                        s.line(").await"); // h.call
                    });
                    s.line("}"); // async fn call

                    s.line("");
                    s.line("pub fn register<'a, T, F, FT>(h: &mut __::Handler<'a, T>, f: F)");
                    s.line("where");
                    s.in_scope(&|s| {
                        s.line("F: Fn(__::Call<T, Params>) -> FT + Sync + Send + 'a,");
                        s.line(
                            "FT: Future<Output = Result<Results, lavish_rpc::Error>> + Send + 'static,",
                        );
                    });
                    s.line("{");
                    s.in_scope(&|s| {
                        s.line("unimplemented!()");
                    });
                    s.line("}"); // fn register
                }
            }
            s.line("}");
            s.line("");
        }
    }
    s.line("}");
    s.line("");
    Ok(())
}
