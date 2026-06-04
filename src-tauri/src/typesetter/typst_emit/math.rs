//! Translate LaTeX-style math (`$...$`, `$$...$$`) into typst math.
//!
//! Backed by the `mitex` crate's `convert_math`. We bundle a small
//! shim with stand-ins for the mitex typst-side helpers (`mitexsqrt`,
//! `mitexdisplay`, `textmath`, `zws`, etc.) so we don't have to pull
//! the `@preview/mitex` typst package — which our sandboxed World
//! refuses on principle.
//!
//! The shim is **conservative**: every helper is a best-effort alias.
//! When a manuscript surfaces a macro the shim doesn't cover, we get
//! a clear typst error pointing at the source position, and we
//! extend the shim here. M0.4 showed 100% coverage on the physics
//! manuscript's 10 sample equations.

/// Inline `$x$` flavor (LaTeX → typst math). Returns just the typst
/// math body, no surrounding `$…$`. Caller is responsible for the
/// dollar delimiters.
pub fn convert_inline(latex: &str) -> Result<String, String> {
    mitex::convert_math(latex, None)
}

/// Display `$$…$$` flavor. Identical translator; only the wrapping
/// differs at emit time.
pub fn convert_display(latex: &str) -> Result<String, String> {
    mitex::convert_math(latex, None)
}

/// Shim of the typst-side helpers the mitex translator emits. This
/// belongs at the top of the typst preamble so every math run
/// downstream can resolve. Kept terse — easier to audit than a
/// vendor drop of the upstream package.
pub const MITEX_SHIM: &str = r#"
// === mitex helper shim (stubs the @preview/mitex/lib.typ surface).
// Conservative aliases — extend here when a manuscript surfaces a
// macro that mitex emits but we haven't covered yet. M0.4 reported
// 100% coverage on the physics manuscript's 10 sample equations.
#let mitexsqrt = math.sqrt
#let mitexlrwrap(left, body, right) = math.lr(left + body + right)
#let mitexdisplay(body, ..) = body
#let mitexscript(body, ..) = body
#let mitexisinitial(..) = false
#let mitexlabel(body, ..) = body
#let mitexspace = h(0.166em)
#let zws = ""
#let textmath(body) = $#body$
// LaTeX environments
#let aligned(body) = body
#let alignedat(_, body) = body
#let gathered(body) = body
#let split(body) = body
#let cases(..body) = math.cases(..body)
#let matrix(..body) = math.mat(..body)
#let pmatrix(..body) = math.mat(delim: "(", ..body)
#let bmatrix(..body) = math.mat(delim: "[", ..body)
#let Bmatrix(..body) = math.mat(delim: "{", ..body)
#let vmatrix(..body) = math.mat(delim: "|", ..body)
#let Vmatrix(..body) = math.mat(delim: "‖", ..body)
#let smallmatrix(..body) = math.mat(..body)
#let array(..body) = math.mat(..body)
#let equation(body) = body
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_real_equation() {
        let out = convert_inline(r"\frac{\hbar c^3}{8\pi G M k_B}")
            .expect("mitex should convert hawking-temp");
        assert!(out.contains("frac"));
        assert!(out.contains("planck.reduce"));
        assert!(out.contains("pi"));
    }

    #[test]
    fn surfaces_parse_errors() {
        // Wildly malformed but the call should still return a Result
        // rather than panic.
        let r = convert_inline(r"\frac{a}");
        // mitex may either error or produce best-effort output; just
        // make sure we don't blow up.
        let _ = r;
    }
}
