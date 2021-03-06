use crate::ast;
use crate::codegen::output::*;
use crate::codegen::Result;

use std::fmt::Write;
use std::fs::File;
use std::time::Instant;

mod prelude;

mod ir;
use ir::*;

pub struct Generator<'a> {
    #[allow(unused)]
    target: ast::RustTarget,

    opts: &'a crate::Opts,
}

impl<'a> Generator<'a> {
    pub fn new(opts: &'a crate::Opts, target: ast::RustTarget) -> Self {
        Self { opts, target }
    }
}

impl<'a> super::Generator for Generator<'a> {
    fn emit_workspace(&self, workspace: &ast::Workspace) -> Result {
        for member in workspace.members.values() {
            self.emit(workspace, member)?;
        }

        {
            let wrapper_name = match self.target.wrapper {
                ast::RustTargetWrapper::None => None,
                ast::RustTargetWrapper::Mod => Some("mod.rs"),
                ast::RustTargetWrapper::Lib => Some("lib.rs"),
            };

            if let Some(wrapper_name) = wrapper_name {
                let wrapper_path = workspace.dir.join(wrapper_name);
                let mut output = Scope::writer(File::create(&wrapper_path)?);
                let mut s = Scope::new(&mut output);
                self.write_prelude(&mut s);

                for member in workspace.members.values() {
                    writeln!(s, "pub mod {};", member.name)?;
                }
            }
        }

        Ok(())
    }
}

impl<'a> Generator<'a> {
    fn write_prelude(&self, s: &mut Scope) {
        s.line("// This file is generated by lavish: DO NOT EDIT");
        s.line("// https://github.com/fasterthanlime/lavish");
        s.lf();
        s.line("#![cfg_attr(rustfmt, rustfmt_skip)]");
        s.line("#![allow(clippy::all, unknown_lints, unused, non_snake_case)]");
        s.lf();
    }

    fn emit(&self, workspace: &ast::Workspace, member: &ast::WorkspaceMember) -> Result {
        let start_instant = Instant::now();

        let output_path = workspace.dir.join(&member.name).join("mod.rs");
        std::fs::create_dir_all(output_path.parent().unwrap())?;
        let mut output = Scope::writer(File::create(&output_path)?);
        let mut scope = Scope::new(&mut output);
        let s = &mut scope;
        self.write_prelude(s);

        let schema = member.schema.as_ref().expect("schema to be parsed");
        let stack = ast::Stack::new(schema);
        let body = stack.anchor(&schema.body);

        {
            s.line("pub use schema::*;");
            s.lf();
        }

        {
            s.write(Protocol { body: body.clone() });
            s.lf();
        }

        {
            write!(s, "pub mod schema").unwrap();
            s.in_block(|s| {
                s.write(Symbols::new(body.clone()));
                write_pair(s, body.clone());
            });
            s.lf();
        }

        let end_instant = Instant::now();
        if self.opts.verbose {
            println!(
                "Generated {:?} in {:?}",
                output_path,
                end_instant.duration_since(start_instant)
            );
        }

        Ok(())
    }
}
