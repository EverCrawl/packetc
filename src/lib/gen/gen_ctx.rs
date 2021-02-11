pub struct GenCtx<'a> {
    pub indentation: String,
    pub out: &'a mut String,
    pub stack: Vec<String>,
}

impl<'a> GenCtx<'a> {
    pub fn new(out: &'a mut String) -> GenCtx {
        GenCtx {
            indentation: String::new(),
            out,
            stack: Vec::new(),
        }
    }

    #[inline]
    pub fn push_indent(&mut self) { self.indentation += "    "; }

    #[inline]
    pub fn pop_indent(&mut self) {
        self.indentation.truncate(if self.indentation.len() < 4 {
            0
        } else {
            self.indentation.len() - 4
        });
    }

    #[inline]
    pub fn push_fname<S: Into<String>>(&mut self, name: S) { self.stack.push(name.into()); }

    #[inline]
    pub fn pop_fname(&mut self) { self.stack.pop(); }

    #[inline]
    pub fn swap_stack(&mut self, other: &mut Vec<String>) { std::mem::swap(&mut self.stack, other); }
}
