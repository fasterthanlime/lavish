use crate::ast;
use std::fmt::{self, Display, Write};
use std::io::{self, BufWriter};

const INDENT_WIDTH: usize = 4;

pub struct Writer<W> {
    writer: W,
}

impl<W> Writer<W>
where
    W: std::io::Write,
{
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    #[allow(unused)]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> fmt::Write for Writer<W>
where
    W: std::io::Write,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write!(self.writer, "{}", s).map_err(|_| fmt::Error {})
    }
}

pub struct Scope<'a> {
    writer: &'a mut fmt::Write,
    indent: usize,
    state: ScopeState,
}

#[derive(PartialEq, Eq)]
enum ScopeState {
    NeedIndent,
    Indented,
}

impl<'a> Scope<'a> {
    pub fn new(writer: &'a mut fmt::Write) -> Self {
        Self {
            writer,
            indent: 0,
            state: ScopeState::NeedIndent,
        }
    }

    pub fn writer<W>(w: W) -> Writer<BufWriter<W>>
    where
        W: io::Write,
    {
        Writer::new(BufWriter::new(w))
    }

    pub fn lf(&mut self) {
        writeln!(self).unwrap();
    }

    pub fn line<D>(&mut self, d: D)
    where
        D: Display,
    {
        self.write(d).lf()
    }

    pub fn write<D>(&mut self, d: D) -> &mut Self
    where
        D: Display,
    {
        write!(self, "{}", d).unwrap();
        self
    }

    pub fn comment(&mut self, comment: &Option<ast::Comment>) {
        if let Some(comment) = comment.as_ref() {
            for line in &comment.lines {
                self.line(format!("/// {}", line))
            }
        }
    }

    pub fn in_scope<F>(&mut self, f: F)
    where
        F: Fn(&mut Scope),
    {
        let mut s = self.scope();
        f(&mut s)
    }

    pub fn in_block<F>(&mut self, f: F)
    where
        F: Fn(&mut Scope),
    {
        self.in_terminated_block("", f);
    }

    pub fn in_terminated_block<F, D>(&mut self, terminator: D, f: F)
    where
        F: Fn(&mut Scope),
        D: Display,
    {
        if !self.fresh_line() {
            self.write(" ");
        }
        self.line("{");
        {
            let mut s = self.scope();
            f(&mut s);
        }
        self.write("}").write(terminator).lf();
    }

    pub fn fmt<F>(writer: &'a mut fmt::Write, f: F) -> std::fmt::Result
    where
        F: Fn(&mut Scope),
    {
        let mut s = Self::new(writer);
        f(&mut s);
        Ok(())
    }

    pub fn scope(&mut self) -> Scope {
        Scope {
            writer: self.writer,
            indent: self.indent + INDENT_WIDTH,
            state: ScopeState::NeedIndent,
        }
    }

    pub fn in_list<F>(&mut self, brackets: Brackets, f: F) -> &mut Self
    where
        F: Fn(&mut List),
    {
        {
            let mut list = List::new(self, ", ", brackets);
            f(&mut list);
        }
        self
    }

    pub fn fresh_line(&self) -> bool {
        self.state == ScopeState::NeedIndent
    }
}

#[derive(Clone, Copy)]
#[allow(unused)]
pub enum Brackets {
    Round,
    Squares,
    Curly,
    Angle,
    None,
}

impl Brackets {
    pub fn pair<'a>(self) -> (&'a str, &'a str) {
        match self {
            Brackets::Round => ("(", ")"),
            Brackets::Squares => ("[", "]"),
            Brackets::Curly => ("{", "}"),
            Brackets::Angle => ("<", ">"),
            Brackets::None => ("", ""),
        }
    }

    pub fn open<'a>(self) -> &'a str {
        self.pair().0
    }

    pub fn close<'a>(self) -> &'a str {
        self.pair().1
    }
}

pub struct List<'a: 'b, 'b> {
    scope: &'b mut Scope<'a>,
    brackets: Brackets,

    empty_list: bool,
    omit_empty: bool,
    separator: String,
}

impl<'a: 'b, 'b> List<'a, 'b> {
    pub fn new<S>(scope: &'b mut Scope<'a>, separator: S, brackets: Brackets) -> Self
    where
        S: Into<String>,
    {
        Self {
            scope,
            brackets,
            empty_list: true,
            omit_empty: false,
            separator: separator.into(),
        }
    }

    pub fn omit_empty(&mut self) {
        self.omit_empty = true;
    }

    pub fn item<D>(&mut self, item: D)
    where
        D: Display,
    {
        let s = &mut self.scope;
        if self.empty_list {
            s.write(self.brackets.open());
            self.empty_list = false
        } else {
            s.write(&self.separator);
        }
        s.write(item);
    }
}

impl<'a, 'b> Drop for List<'a, 'b> {
    fn drop(&mut self) {
        if self.empty_list {
            if self.omit_empty {
                return;
            }

            self.scope
                .write(self.brackets.open())
                .write(self.brackets.close());
        } else {
            self.scope.write(self.brackets.close());
        }
    }
}

impl<'a> fmt::Write for Scope<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for (i, token) in s.split('\n').enumerate() {
            // each token is a string slice without newlines
            if i > 0 {
                // each token after the first one is preceded by a newline,
                // so let's write it out
                writeln!(self.writer).map_err(|_| fmt::Error {})?;
                self.state = ScopeState::NeedIndent;
            }

            if token.is_empty() {
                continue;
            }

            match self.state {
                ScopeState::NeedIndent => {
                    write!(self.writer, "{}", " ".repeat(self.indent))
                        .map_err(|_| fmt::Error {})?;
                    self.state = ScopeState::Indented
                }
                ScopeState::Indented => {}
            }
            write!(self.writer, "{}", token).map_err(|_| fmt::Error {})?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Scope;
    use netbuf::Buf;
    use std::fmt::Write;

    #[test]
    fn test_scope() -> Result<(), Box<dyn std::error::Error + 'static>> {
        let mut buf = Buf::new();
        {
            let mut w = super::Writer::new(&mut buf);
            let mut s = Scope::new(&mut w);
            writeln!(s, "fn sample() {{")?;
            {
                let mut s = s.scope();
                writeln!(s, "let a = {{")?;
                {
                    let mut s = s.scope();
                    let val = 7;
                    writeln!(s, "let tmp = {val};", val = val)?;
                    writeln!(s, "// a blank line follows")?;
                    writeln!(s)?;
                    writeln!(s, "tmp + 3")?;
                }
                writeln!(s, "}};")?;
            }
            writeln!(s, "}}")?;
        }

        let s = std::str::from_utf8(buf.as_ref()).unwrap();
        assert_eq!(
            s,
            r#"fn sample() {
    let a = {
        let tmp = 7;
        // a blank line follows

        tmp + 3
    };
}
"#,
        );
        Ok(())
    }
}
