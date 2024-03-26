use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote_spanned, ToTokens};
use syn::{
    parse_macro_input, parse_quote,
    spanned::Spanned,
    visit_mut::{self, VisitMut},
    BinOp, Error, Expr, ExprBinary, Result, Token,
};

#[proc_macro]
pub fn vectorize(input: TokenStream) -> TokenStream {
    struct VectorizeArgs {
        len: Expr,
        _comma: Token![,],
        expr: Expr,
    }

    impl syn::parse::Parse for VectorizeArgs {
        fn parse(input: syn::parse::ParseStream) -> Result<Self> {
            Ok(Self {
                len: input.parse()?,
                _comma: input.parse()?,
                expr: input.parse()?,
            })
        }
    }

    struct Vectorize(Option<Error>, Expr);

    impl Vectorize {
        fn error<T: std::fmt::Display>(&mut self, span: Span, message: T) {
            let error = Error::new(span, message);
            if let Some(e) = &mut self.0 {
                e.combine(error);
            } else {
                self.0 = Some(error);
            }
        }
    }

    impl VisitMut for Vectorize {
        fn visit_expr_mut(&mut self, i: &mut Expr) {
            let len = &self.1;
            match i {
                Expr::If(e) => {
                    if let Some(else_branch) = &e.else_branch {
                        let cond = &e.cond;
                        let then_branch = &e.then_branch;
                        let else_branch = &else_branch.1;
                        *i = parse_quote! {
                            (#cond).select(#then_branch, #else_branch)
                        };
                    } else {
                        self.error(e.if_token.span, "vectorizing `if` needs a matching `else`");
                    }
                }
                Expr::Binary(e) => {
                    fn cmp(e: &ExprBinary, func: &str) -> Expr {
                        let func = str::parse::<proc_macro2::TokenStream>(func).unwrap();
                        let func = quote_spanned! {
                            e.op.span() => core::simd::cmp::#func
                        };
                        let left = &e.left;
                        let right = &e.right;
                        parse_quote! {
                            #func(#left, #right)
                        }
                    }

                    match e.op {
                        BinOp::Eq(_) => *i = cmp(e, "SimdPartialEq::simd_eq"),
                        BinOp::Ne(_) => *i = cmp(e, "SimdPartialEq::simd_ne"),
                        BinOp::Gt(_) => *i = cmp(e, "SimdPartialOrd::simd_gt"),
                        BinOp::Ge(_) => *i = cmp(e, "SimdPartialOrd::simd_ge"),
                        BinOp::Lt(_) => *i = cmp(e, "SimdPartialOrd::simd_lt"),
                        BinOp::Le(_) => *i = cmp(e, "SimdPartialOrd::simd_le"),
                        _ => (),
                    }
                }
                Expr::Lit(e) => {
                    *i = parse_quote! { core::simd::Simd::<_, #len>::splat(#e) };
                    // don't recurse, or we'll expand this infinitely
                    return;
                }
                Expr::Cast(e) => {
                    self.visit_expr_mut(&mut e.expr);

                    let val = &e.expr;
                    let ty = &e.ty;

                    *i = parse_quote! {
                        {
                            let _val = #val;
                            {
                                use core::simd::prelude::*;
                                _val.cast::<#ty>()
                            }
                        }
                    };

                    // don't expand #ty
                    return;
                }
                Expr::Macro(e) => {
                    if e.mac.path == parse_quote!(scalar) {
                        let expr = &e.mac.tokens;
                        *i = parse_quote!(Simd::<_, #len>::splat(#expr));

                        // don't expand contents
                        return;
                    } else if e.mac.path == parse_quote!(verbatim) {
                        let expr = &e.mac.tokens;
                        *i = parse_quote!(#expr);

                        // don't expand contents
                        return;
                    }
                }
                _ => (),
            }

            // visit contents
            visit_mut::visit_expr_mut(self, i)
        }

        fn visit_type_mut(&mut self, i: &mut syn::Type) {
            let len = &self.1;
            *i = parse_quote! {
                core::simd::Simd<#i, #len>
            };
        }
    }

    let input = parse_macro_input!(input as VectorizeArgs);
    let mut expr = input.expr;
    let mut vectorize = Vectorize(None, input.len);
    vectorize.visit_expr_mut(&mut expr);

    if let Some(error) = vectorize.0 {
        error.into_compile_error().into()
    } else {
        expr.into_token_stream().into()
    }
}
