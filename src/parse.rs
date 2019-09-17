use proc_macro2::{Span, TokenStream};
use syn::{Expr, Ident, Lit, LitStr, Meta, MetaList, NestedMeta, Token,
          parse::{Error, Parse, ParseStream, Result},
          punctuated::Punctuated,
          spanned::Spanned};

use cfg_if::cfg_if;
use quote::ToTokens;

use crate::{modifiers::FixtureModifiers, modifiers::RsTestModifiers};

#[derive(Debug)]
pub enum ParametrizeItem {
    Fixture(Fixture),
    CaseArgName(Ident),
    TestCase(TestCase),
}

#[derive(Default, Debug)]
pub struct ParametrizeData {
    pub data: Vec<ParametrizeItem>,
}

impl ParametrizeData {
    pub fn args(&self) -> impl Iterator<Item=&Ident> {
        self.data.iter()
            .filter_map(|it|
                match it {
                    ParametrizeItem::CaseArgName(ref arg) => Some(arg),
                    _ => None
                }
            )
    }

    pub fn cases(&self) -> impl Iterator<Item=&TestCase> {
        self.data.iter()
            .filter_map(|it|
                match it {
                    ParametrizeItem::TestCase(ref case) => Some(case),
                    _ => None
                }
            )
    }

    pub fn fixtures(&self) -> impl Iterator<Item=&Fixture> {
        self.data.iter()
            .filter_map(|it|
                match it {
                    ParametrizeItem::Fixture(ref fixture) => Some(fixture),
                    _ => None
                }
            )
    }
}

#[derive(Default, Debug)]
/// Parametrize
pub struct ParametrizeInfo {
    pub data: ParametrizeData,
    pub modifiers: Modifiers,
}

#[derive(Debug)]
/// A test case instance data. Contains a list of arguments. It is parsed by parametrize
/// attributes.
pub struct TestCase {
    pub args: Vec<CaseArg>,
    pub description: Option<Ident>,
}

impl Parse for TestCase {
    fn parse(input: ParseStream) -> Result<Self> {
        let case: Ident = input.parse()?;
        if case == "case" {
            let mut description = None;
            if input.peek(Token![::]) {
                let _ = input.parse::<Token![::]>();
                description = Some(input.parse()?);
            }
            let content;
            let _ = syn::parenthesized!(content in input);
            let args = Punctuated::<CaseArg, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            Ok(TestCase { args, description })
        } else {
            Err(Error::new(case.span(), "expected a test case"))
        }
    }
}

impl TestCase {
    pub fn span_start(&self) -> Span {
        self.args.first().map(|arg| arg.span()).unwrap_or(Span::call_site())
    }
    pub fn span_end(&self) -> Span {
        self.args.last().map(|arg| arg.span()).unwrap_or(Span::call_site())
    }
}

#[derive(Debug, Clone)]
/// A test case's argument as an expression that can be assigned.
pub struct CaseArg {
    expr: Expr,
}

impl CaseArg {
    pub fn new(expr: Expr) -> Self {
        Self { expr }
    }
}

#[cfg(test)]
impl PartialEq for CaseArg {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

// To Enable Spanned trait
impl ToTokens for CaseArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.expr.to_tokens(tokens);
    }
}

impl From<Expr> for CaseArg {
    fn from(expr: Expr) -> Self {
        CaseArg::new(expr)
    }
}


struct UnwrapRustCode(Expr);

impl Parse for UnwrapRustCode {
    fn parse(input: ParseStream) -> Result<Self> {
        let nested: NestedMeta = input.parse()?;
        Self::report_deprecated(&nested);
        let arg = Self::get_unwrap(&nested)?;
        arg.nested.first()
            .map(|m| *m.value())
            .and_then(Self::nested_meta_literal_str)
            .ok_or(syn::Error::new_spanned(&nested,
                                           &format!("Invalid {} argument", arg.ident)))
            .and_then(|lit| lit.parse())
            .map(UnwrapRustCode)
    }
}

impl UnwrapRustCode {
    const UNWRAP_NAME: &'static str = "Unwrap";

    fn peek(input: ParseStream) -> bool {
        input.fork().parse::<NestedMeta>().map(|nested|
            Self::get_unwrap(&nested).is_ok()
        ).unwrap_or(false)
    }

    fn get_unwrap(nested: &NestedMeta) -> Result<&MetaList> {
        match nested {
            NestedMeta::Meta(Meta::List(ref arg)) if arg.ident == Self::UNWRAP_NAME => Ok(arg),
            _ => return Err(Error::new(nested.span(), "Not a valid string rust code"))
        }
    }

    fn report_deprecated(nested: &NestedMeta) {
        cfg_if! {
        if #[cfg(use_proc_macro_diagnostic)] {
            fn inner(nested: &NestedMeta) {
                nested.span()
                    .unwrap()
                    .warning("Deprecated: Case argument accepts arbitrary rust code now.")
                    .emit();
            }
        } else {
            fn inner(nested: &NestedMeta) {
                match nested {
                    NestedMeta::Meta(Meta::List(arg)) => {
                        arg.nested.first()
                            .map(|m| *m.value())
                            .and_then(UnwrapRustCode::nested_meta_literal_str)
                            .map(|content| {
                                eprintln!(r#"{}("<code>") is deprecated. Case argument accepts arbitrary rust code now."#,
                                          arg.ident);
                                content
                            })
                            .unwrap();
                    }
                    _ => { unreachable!() }
                }
            }
        }
        }
        inner(nested);
    }

    fn nested_meta_literal_str(nested: &NestedMeta) -> Option<&LitStr> {
        match nested {
            NestedMeta::Literal(Lit::Str(lit)) => Some(lit),
            _ => None
        }
    }
}

impl Parse for CaseArg {
    fn parse(input: ParseStream) -> Result<Self> {
        if UnwrapRustCode::peek(input) {
            Ok(CaseArg::new(input.parse::<UnwrapRustCode>()?.0))
        } else {
            input.parse()
                .map(CaseArg::new)
                .map_err(|e| Error::new(
                    e.span(),
                    format!("Cannot parse due {}", e),
                )
                )
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct Modifiers {
    pub modifiers: Vec<RsTestAttribute>
}

impl Parse for Modifiers {
    fn parse(input: ParseStream) -> Result<Self> {
        let vars = Punctuated::<RsTestAttribute, Token![::]>::parse_terminated(input)?;
        Ok(Modifiers {
            modifiers: vars.into_iter().collect(),
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum RsTestAttribute {
    Attr(Ident),
    Tagged(Ident, Vec<Ident>),
    Type(Ident, syn::Type),
}

fn no_literal_nested(nested: NestedMeta) -> Result<Meta> {
    match nested {
        NestedMeta::Meta(m) => Ok(m),
        NestedMeta::Literal(l) => Err(Error::new(l.span(), "Unexpected literal"))
    }
}

fn just_word_meta(meta: Meta) -> Result<Ident> {
    match meta {
        Meta::Word(ident) => Ok(ident),
        other => Err(Error::new(other.span(), "Should be an ident"))
    }
}

impl Parse for RsTestAttribute {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek2(Token![<]) {
            let tag = input.parse()?;
            let _open = input.parse::<Token![<]>()?;
            let inner = input.parse()?;
            let _close = input.parse::<Token![>]>()?;
            Ok(RsTestAttribute::Type(tag, inner))
        } else {
            let meta = no_literal_nested(NestedMeta::parse(input)?)?;
            use Meta::*;
            match meta {
                Word(ident) => Ok(RsTestAttribute::Attr(ident)),
                List(l) =>
                    Ok(RsTestAttribute::Tagged(l.ident,
                                               l.nested.into_iter()
                                                   .map(no_literal_nested)
                                                   .collect::<Result<Vec<Meta>>>()?
                                                   .into_iter().map(just_word_meta)
                                                   .collect::<Result<Vec<Ident>>>()?,
                    )),
                NameValue(nv) => Err(Error::new(nv.span(), "Invalid attribute"))
            }
        }
    }
}

impl Parse for ParametrizeItem {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.fork().parse::<TestCase>().is_ok() {
            input.parse::<TestCase>().map(ParametrizeItem::TestCase)
        } else if input.fork().parse::<Fixture>().is_ok() {
            input.parse::<Fixture>().map(ParametrizeItem::Fixture)
        } else if input.fork().parse::<Ident>().is_ok() {
            input.parse::<Ident>().map(ParametrizeItem::CaseArgName)
        } else {
            Err(syn::Error::new(Span::call_site(), "Cannot parse parametrize info"))
        }
    }
}

fn parse_vector_trailing<T, P>(input: ParseStream) -> Result<Vec<T>>
    where
        T: Parse,
        P: syn::token::Token + Parse
{
    Ok(
        Punctuated::<Option<T>, P>::parse_separated_nonempty_with(
            input, |input_tokens|
                if input_tokens.is_empty() {
                    Ok(None)
                } else {
                    T::parse(input_tokens).map(|inner| Some(inner))
                },
        )?.into_iter()
            .filter_map(|it| it)
            .collect()
    )
}

impl Parse for ParametrizeData {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(ParametrizeData { data: parse_vector_trailing::<_, Token![,]>(input)? })
    }
}

impl Parse for ParametrizeInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse a `ParametrizeData` and then look for `Modifiers` after `::` token if any.
        // Example
        //  |-    u,a,d
        //  |-    case(42, r("A{}"), Unwrap("D{}"))
        //  |  ::trace::notrace(a))
        //  |    ^^^^^^^^^^^^^^^^^^ Modifiers
        //  |______________________ ParametrizeData
        Ok(
            ParametrizeInfo {
                data: input.parse()?,
                modifiers: input.parse::<Token![::]>()
                    .or_else(|_| Ok(Default::default()))
                    .and_then(|_| input.parse())?,
            }
        )
    }
}

pub struct ValueList {
    pub arg: Ident,
    pub values: Vec<CaseArg>,
}

impl Parse for ValueList {
    fn parse(input: ParseStream) -> Result<Self> {
        let arg = input.parse()?;
        let _to: Token![=>] = input.parse()?;
        let content;
        let paren = syn::bracketed!(content in input);
        let values = content
            .parse_terminated::<_, Token![,]>(Parse::parse)?
            .into_iter()
            .collect();

        let ret = Self {
            arg,
            values,
        };
        if ret.values.len() == 0 {
            Err(syn::Error::new(paren.span, "Values list should not be empty"))
        } else {
            Ok(ret)
        }
    }
}

pub enum MatrixItem {
    ValueList(ValueList),
    Fixture(Fixture),
}

impl Parse for MatrixItem {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek2(Token![=>]) {
            input.parse::<ValueList>().map(Self::from)
        } else if input.fork().parse::<Fixture>().is_ok() {
            input.parse::<Fixture>().map(Self::from)
        } else {
            Err(syn::Error::new(Span::call_site(), "Cannot parse matrix info"))
        }
    }
}

impl From<ValueList> for MatrixItem {
    fn from(value_list: ValueList) -> Self {
        MatrixItem::ValueList(value_list)
    }
}

impl From<Fixture> for MatrixItem {
    fn from(fixture: Fixture) -> Self {
        MatrixItem::Fixture(fixture)
    }
}

#[derive(Default)]
pub struct MatrixValues(pub Vec<MatrixItem>);

impl MatrixValues {
    pub fn list_values(&self) -> impl Iterator<Item=&ValueList> {
        self.0.iter().filter_map(|mv|
            match mv {
                MatrixItem::ValueList(ref value_list) => Some(value_list),
                _ => None
            }
        )
    }

    pub fn fixtures(&self) -> impl Iterator<Item=&Fixture> {
        self.0.iter().filter_map(|mv|
            match mv {
                MatrixItem::Fixture(ref fixture) => Some(fixture),
                _ => None
            }
        )
    }
}

#[derive(Default)]
pub struct MatrixInfo {
    pub args: MatrixValues,
    pub modifiers: Modifiers,
}

#[allow(dead_code)]
fn drain_stream(input: ParseStream) {
    // JUST TO SKIP ALL
    let _ = input.step(|cursor| {
        let mut rest = *cursor;
        while let Some((_, next)) = rest.token_tree() {
            rest = next
        };
        Ok(((), rest))
    });
}

impl Parse for MatrixInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(
            MatrixInfo {
                args: parse_vector_trailing::<_, Token![,]>(input)
                    .map(MatrixValues)?,
                modifiers: input.parse::<Token![::]>()
                    .or_else(|_| Ok(Default::default()))
                    .and_then(|_| input.parse())?,
            }
        )
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Fixture {
    pub name: Ident,
    pub positional: Vec<syn::Expr>,
}

impl Fixture {
    pub fn new(name: Ident, positional: Vec<syn::Expr>) -> Self {
        Self { name, positional }
    }
}

impl Parse for Fixture {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        let content;
        let _ = syn::parenthesized!(content in input);
        let positional = Punctuated::<syn::Expr, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(
            Self::new(name, positional)
        )
    }
}

#[derive(PartialEq, Debug)]
pub enum RsTestItem {
    Fixture(Fixture)
}

impl RsTestItem {
    pub fn name(&self) -> &Ident {
        match self {
            RsTestItem::Fixture(Fixture { ref name, .. }) => name
        }
    }
}

impl From<Fixture> for RsTestItem {
    fn from(f: Fixture) -> Self {
        RsTestItem::Fixture(f)
    }
}

impl Parse for RsTestItem {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse().map(RsTestItem::Fixture)
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct RsTestData {
    pub items: Vec<RsTestItem>
}

impl RsTestData {
    pub fn fixtures(&self) -> impl Iterator<Item=&Fixture> {
        self.items.iter().filter_map(|it|
            match it {
                RsTestItem::Fixture(ref fixture) => Some(fixture),
                #[allow(unreachable_patterns)]
                _ => None
            }
        )
    }
}

impl Parse for RsTestData {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![::]) {
            Ok(Default::default())
        } else {
            Ok(Self { items: parse_vector_trailing::<_, Token![,]>(input)? })
        }
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct RsTestInfo {
    pub(crate) data: RsTestData,
    pub(crate) modifiers: RsTestModifiers,
}

impl Parse for RsTestModifiers {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(input.parse::<Modifiers>()?.into())
    }
}

impl Parse for RsTestInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(
            if input.is_empty() {
                Default::default()
            } else {
                Self {
                    data: input.parse()?,
                    modifiers: input.parse::<Token![::]>()
                        .or_else(|_| Ok(Default::default()))
                        .and_then(|_| input.parse())?,
                }
            }
        )
    }
}

#[derive(PartialEq, Debug)]
pub enum FixtureItem {
    Fixture(Fixture)
}

impl FixtureItem {
    pub fn name(&self) -> &Ident {
        match self {
            FixtureItem::Fixture(Fixture { ref name, .. }) => name
        }
    }
}

impl From<Fixture> for FixtureItem {
    fn from(f: Fixture) -> Self {
        FixtureItem::Fixture(f)
    }
}

impl Parse for FixtureItem {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse().map(FixtureItem::Fixture)
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct FixtureData {
    pub items: Vec<FixtureItem>
}

impl FixtureData {
    pub fn fixtures(&self) -> impl Iterator<Item=&Fixture> {
        self.items.iter()
            .filter_map(|f|
                match f {
                    FixtureItem::Fixture(ref fixture) => Some(fixture),
                    #[allow(unreachable_patterns)]
                    _ => None,
                }
            )
    }
}

impl Parse for FixtureData {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![::]) {
            Ok(Default::default())
        } else {
            Ok(Self { items: parse_vector_trailing::<_, Token![,]>(input)? })
        }
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct FixtureInfo {
    pub(crate) data: FixtureData,
    pub(crate) modifiers: FixtureModifiers,
}

impl Parse for FixtureModifiers {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(input.parse::<Modifiers>()?.into())
    }
}

impl Parse for FixtureInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(
            if input.is_empty() {
                Default::default()
            } else {
                Self {
                    data: input.parse()?,
                    modifiers: input.parse::<Token![::]>()
                        .or_else(|_| Ok(Default::default()))
                        .and_then(|_| input.parse())?,
                }
            }
        )
    }
}

#[cfg(test)]
pub mod should {
    #[allow(unused_imports)]
    use pretty_assertions::assert_eq;
    use proc_macro2::TokenTree;
    use syn::{ItemFn, parse2, parse_str};
    use itertools::Itertools;

    use quote::quote;

    use super::*;

    fn parse_meta<T: syn::parse::Parse, S: AsRef<str>>(test_case: S) -> T {
        let to_parse = format!(r#"
        #[{}]
        fn to_parse() {{}}
        "#, test_case.as_ref());

        let item_fn = parse_str::<ItemFn>(&to_parse).unwrap();

        let tokens = quote!(
            #item_fn
        );

        let tt = tokens.into_iter().skip(1).next().unwrap();

        if let TokenTree::Group(g) = tt {
            let ts = g.stream();
            parse2::<T>(ts).unwrap()
        } else {
            panic!("Cannot find group in {:#?}", tt)
        }
    }

    #[macro_export]
    macro_rules! to_args {
        ($e:expr) => {$e.iter()
                        .map(|s| s as & dyn AsRef<str>)
                        .map(case_arg)
                        .collect_vec()
                        };
    }
    #[macro_export]
    macro_rules! to_exprs {
        ($e:expr) => {$e.iter().map(|s| expr(s)).collect::<Vec<_>>()};
    }
    #[macro_export]
    macro_rules! to_strs {
        ($e:expr) => {$e.iter().map(ToString::to_string).collect::<Vec<_>>()};
    }

    pub fn ident<S: AsRef<str>>(s: S) -> syn::Ident {
        parse_str::<syn::Ident>(s.as_ref()).unwrap()
    }

    pub fn expr<S: AsRef<str>>(s: S) -> syn::Expr {
        parse_str::<syn::Expr>(s.as_ref()).unwrap()
    }

    pub fn fixture(name: impl AsRef<str>, args: Vec<&str>) -> Fixture {
        Fixture::new(ident(name), to_exprs!(args))
    }

    pub fn case_arg<S: AsRef<str>>(s: S) -> CaseArg {
        CaseArg::new(expr(s))
    }

    pub fn values_list<S: AsRef<str>>(arg: &str, values: &[S]) -> ValueList {
        ValueList {
            arg: ident(arg),
            values: values.into_iter().map(|s| case_arg(s)).collect(),
        }
    }

    trait ExtractArgs {
        fn args(&self) -> Vec<CaseArg>;
    }

    impl ExtractArgs for TestCase {
        fn args(&self) -> Vec<CaseArg> {
            self.args.iter().cloned().collect()
        }
    }

    impl ExtractArgs for ValueList {
        fn args(&self) -> Vec<CaseArg> {
            self.values.iter().cloned().collect()
        }
    }

    impl RsTestAttribute {
        fn attr<S: AsRef<str>>(s: S) -> Self {
            RsTestAttribute::Attr(ident(s))
        }

        fn tagged<SI: AsRef<str>, SA: AsRef<str>>(tag: SI, attrs: Vec<SA>) -> Self {
            RsTestAttribute::Tagged(
                ident(tag),
                attrs.into_iter()
                    .map(|a| ident(a))
                    .collect(),
            )
        }

        fn typed<S: AsRef<str>, T: AsRef<str>>(tag: S, inner: T) -> Self {
            RsTestAttribute::Type(
                ident(tag),
                parse_str(inner.as_ref()).unwrap(),
            )
        }
    }

    impl From<Vec<RsTestItem>> for RsTestData {
        fn from(fixtures: Vec<RsTestItem>) -> Self {
            Self { items: fixtures }
        }
    }

    impl From<Vec<FixtureItem>> for FixtureData {
        fn from(fixtures: Vec<FixtureItem>) -> Self {
            Self { items: fixtures }
        }
    }

    mod parse_fixture_values {
        use super::*;
        use super::assert_eq;

        fn parse_fixtures<S: AsRef<str>>(fixtures: S) -> RsTestData {
            parse_meta(fixtures)
        }

        #[test]
        fn one_arg() {
            let fixtures = parse_fixtures("my_fixture(42)");

            let expected = RsTestData {
                items: vec![
                    Fixture::new(ident("my_fixture"), vec![expr("42")]).into()
                ]
            };

            assert_eq!(expected, fixtures);
        }
    }

    mod parse_fixture {
        use super::*;
        use super::assert_eq;

        fn parse_fixture<S: AsRef<str>>(fixture_data: S) -> FixtureInfo {
            parse_meta(fixture_data)
        }

        #[test]
        fn happy_path() {
            let data = parse_fixture(r#"my_fixture(42, "other"), other(vec![42])
                    :: trace :: no_trace(some)"#);

            let expected = FixtureInfo {
                data: vec![
                    fixture("my_fixture", vec!["42", r#""other""#]).into(),
                    fixture("other", vec!["vec![42]"]).into(),
                ].into(),
                modifiers: Modifiers {
                    modifiers: vec![
                        RsTestAttribute::attr("trace"),
                        RsTestAttribute::tagged("no_trace", vec!["some"])
                    ]
                }.into(),
            };

            assert_eq!(expected, data);
        }

        #[test]
        fn empty_fixtures() {
            let data = parse_fixture(r#"::trace::no_trace(some)"#);

            let expected = FixtureInfo {
                modifiers: Modifiers {
                    modifiers: vec![
                        RsTestAttribute::attr("trace"),
                        RsTestAttribute::tagged("no_trace", vec!["some"])
                    ]
                }.into(),
                ..Default::default()
            };

            assert_eq!(expected, data);
        }

        #[test]
        fn empty_modifiers() {
            let data = parse_fixture(r#"my_fixture(42, "other")"#);

            let expected = FixtureInfo {
                data: vec![
                    fixture("my_fixture", vec!["42", r#""other""#]).into(),
                ].into(),
                ..Default::default()
            };

            assert_eq!(expected, data);
        }

        #[test]
        fn should_accept_trailing_comma() {
            let fixtures = vec![
                parse_fixture(r#"first(42),"#),
                // See #52
            //    parse_fixture(r#"fixture(42, "other"), :: trace"#),
            ];

            for f in fixtures {
                assert_eq!(1, f.data.fixtures().count());
            }
        }
    }

    mod parse_rstest {
        use super::*;
        use super::assert_eq;

        fn parse_rstest<S: AsRef<str>>(rstest_data: S) -> RsTestInfo {
            parse_meta(rstest_data)
        }

        #[test]
        fn happy_path() {
            let data = parse_rstest(r#"my_fixture(42, "other"), other(vec![42])
                    :: trace :: no_trace(some)"#);

            let expected = RsTestInfo {
                data: vec![
                    fixture("my_fixture", vec!["42", r#""other""#]).into(),
                    fixture("other", vec!["vec![42]"]).into(),
                ].into(),
                modifiers: Modifiers {
                    modifiers: vec![
                        RsTestAttribute::attr("trace"),
                        RsTestAttribute::tagged("no_trace", vec!["some"])
                    ]
                }.into(),
            };

            assert_eq!(expected, data);
        }

        #[test]
        fn empty_fixtures() {
            let data = parse_rstest(r#"::trace::no_trace(some)"#);

            let expected = RsTestInfo {
                modifiers: Modifiers {
                    modifiers: vec![
                        RsTestAttribute::attr("trace"),
                        RsTestAttribute::tagged("no_trace", vec!["some"])
                    ]
                }.into(),
                ..Default::default()
            };

            assert_eq!(expected, data);
        }

        #[test]
        fn empty_modifiers() {
            let data = parse_rstest(r#"my_fixture(42, "other")"#);

            let expected = RsTestInfo {
                data: vec![
                    fixture("my_fixture", vec!["42", r#""other""#]).into(),
                ].into(),
                ..Default::default()
            };

            assert_eq!(expected, data);
        }
    }

    mod parse_modifiers {
        use super::*;
        use super::assert_eq;

        fn parse_modifiers<S: AsRef<str>>(modifiers: S) -> Modifiers {
            parse_meta(modifiers)
        }

        #[test]
        fn one_simple_ident() {
            let modifiers = parse_modifiers("my_ident");

            let expected = Modifiers {
                modifiers: vec![
                    RsTestAttribute::attr("my_ident")
                ]
            };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn one_simple_group() {
            let modifiers = parse_modifiers("group_tag(first, second)");

            let expected = Modifiers {
                modifiers: vec![
                    RsTestAttribute::tagged("group_tag", vec!["first", "second"])
                ]
            };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn one_simple_type() {
            let modifiers = parse_modifiers("type_tag<(u32, T, (String, i32))>");

            let expected = Modifiers {
                modifiers: vec![
                    RsTestAttribute::typed("type_tag", "(u32, T, (String, i32))")
                ]
            };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn integrated() {
            let modifiers = parse_modifiers(r#"
            simple :: tagged(first, second) :: type_tag<(u32, T, (std::string::String, i32))> :: more_tagged(a,b)"#);

            let expected = Modifiers {
                modifiers: vec![
                    RsTestAttribute::attr("simple"),
                    RsTestAttribute::tagged("tagged", vec!["first", "second"]),
                    RsTestAttribute::typed("type_tag", "(u32, T, (std::string::String, i32))"),
                    RsTestAttribute::tagged("more_tagged", vec!["a", "b"]),
                ]
            };

            assert_eq!(expected, modifiers);
        }
    }

    fn literal_expressions_str() -> Vec<&'static str> {
        vec!["42", "42isize", "1.0", "-1", "-1.0", "true", "1_000_000u64", "0b10100101u8",
             r#""42""#, "b'H'"]
    }

    mod parse_test_case {
        use super::*;
        use super::assert_eq;

        fn parse_test_case<S: AsRef<str>>(test_case: S) -> TestCase {
            parse_meta(test_case)
        }

        #[test]
        fn two_literal_args() {
            let test_case = parse_test_case(r#"case(42, "value")"#);
            let args = test_case.args();

            let expected = to_args!(["42", r#""value""#]);

            assert_eq!(expected, args);
        }

        #[test]
        fn some_literals() {
            let args_expressions = literal_expressions_str();
            let test_case = parse_test_case(&format!("case({})", args_expressions.join(", ")));
            let args = test_case.args();

            assert_eq!(to_args!(args_expressions), args);
        }

        #[test]
        fn raw_code() {
            let test_case = parse_test_case(r#"case(vec![1,2,3])"#);
            let args = test_case.args();

            assert_eq!(to_args!(["vec![1, 2, 3]"]), args);
        }

        #[test]
        #[should_panic(expected = r#"Cannot parse due"#)]
        fn raw_code_with_parsing_error() {
            parse_test_case(r#"case(some:<>(1,2,3))"#);
        }

        #[test]
        fn should_read_test_description_if_any() {
            let test_case = parse_test_case(r#"case::this_test_description(42)"#);
            let args = test_case.args();

            assert_eq!("this_test_description", &test_case.description.unwrap().to_string());
            assert_eq!(to_args!(["42"]), args);
        }

        #[test]
        fn should_read_test_description_also_with_more_args() {
            let test_case = parse_test_case(r#"case :: this_test_description (42, 24)"#);
            let args = test_case.args();

            assert_eq!("this_test_description", &test_case.description.unwrap().to_string());
            assert_eq!(to_args!(["42", "24"]), args);
        }

        #[test]
        fn should_parse_arbitrary_rust_code_as_expression() {
            let test_case = parse_test_case(r##"
            case(42, -42,
            pippo("pluto"),
            Vec::new(),
            String::from(r#"prrr"#),
            {
                let mut sum=0;
                for i in 1..3 {
                    sum += i;
                }
                sum
            },
            vec![1,2,3]
            )"##);
            let args = test_case.args();

            assert_eq!(to_args!(["42", "-42", r#"pippo("pluto")"#, "Vec::new()",
            r##"String::from(r#"prrr"#)"##, r#"{let mut sum=0;for i in 1..3 {sum += i;}sum}"#,
            "vec![1,2,3]"]), args);
        }
    }

    mod parse_parametrize {
        use pretty_assertions::assert_eq;

        use super::*;

        fn parse_data<S: AsRef<str>>(test_case: S) -> ParametrizeData {
            parse_meta(test_case)
        }

        #[test]
        fn one_simple_case_one_arg() {
            let data = parse_data(r#"arg, case(42)"#);

            let args = data.args().collect::<Vec<_>>();
            let cases = data.cases().collect::<Vec<_>>();

            assert_eq!(1, args.len());
            assert_eq!(1, cases.len());
            assert_eq!("arg", &args[0].to_string());
            assert_eq!(to_args!(["42"]), cases[0].args())
        }

        #[test]
        fn happy_path() {
            let data = parse_data(r#"
                my_fixture(42,"foo"),
                arg1, arg2, arg3,
                case(1,2,3),
                case(11,12,13),
                case(21,22,23)
                "#);

            let fixtures = data.fixtures()
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(vec![fixture("my_fixture", vec!["42", r#""foo""#])], fixtures);
            assert_eq!(to_strs!(vec!["arg1", "arg2", "arg3"]), data.args()
                .map(ToString::to_string)
                .collect::<Vec<_>>());
            let cases = data.cases().collect::<Vec<_>>();
            assert_eq!(3, cases.len());
            assert_eq!(to_args!(["1", "2", "3"]), cases[0].args());
            assert_eq!(to_args!(["11", "12", "13"]), cases[1].args());
            assert_eq!(to_args!(["21", "22", "23"]), cases[2].args());
        }

        #[test]
        fn should_accept_comma_at_the_end_of_cases() {
            let data = parse_data(r#"
                arg,
                case(42),
                "#);

            let args = data.args().collect::<Vec<_>>();
            let cases = data.cases().collect::<Vec<_>>();

            assert_eq!(1, args.len());
            assert_eq!(1, cases.len());
            assert_eq!("arg", &args[0].to_string());
            assert_eq!(to_args!(["42"]), cases[0].args())
        }

        #[test]
        fn integrated_1() {
            let data = parse_data(r#"
                u,a,d,
                case(42, A{}, D{})
                "#);

            assert_eq!(to_strs!(vec!["u", "a", "d"]), data.args()
                .map(ToString::to_string)
                .collect::<Vec<_>>());
            let cases = data.cases().collect::<Vec<_>>();
            assert_eq!(1, cases.len());
            assert_eq!(to_args!(["42", "A{}", "D{}"]), cases[0].args());
        }

        #[test]
        fn integrated_2() {
            let data = parse_data(r#"
                ret,
                case::should_success(Ok(())),
                case::should_fail(Err("Return Error"))
                "#);

            assert_eq!(to_strs!(vec!["ret"]), data.args()
                .map(ToString::to_string)
                .collect::<Vec<_>>());
            let cases = data.cases().collect::<Vec<_>>();
            assert_eq!(2, cases.len());
            assert_eq!(to_args!(["Ok(())"]), cases[0].args());
            assert_eq!("should_success", &cases[0].description.as_ref().unwrap().to_string());
            assert_eq!(to_args!([r#"Err("Return Error")"#]), cases[1].args());
            assert_eq!("should_fail", &cases[1].description.as_ref().unwrap().to_string());
        }

        #[test]
        #[should_panic]
        fn should_not_accept_invalid_separator_from_args_and_cases() {
            parse_data(r#"
                ret
                case::should_success(Ok(())),
                case::should_fail(Err("Return Error"))
                "#);
        }

        #[test]
        fn should_accept_any_order() {
            let data = parse_data(r#"
                u,
                case(42, A{}, D{}),
                a,
                case(43, A{}, D{}),
                the_fixture(42),
                d
                "#);

            assert_eq!(to_strs!(vec!["u", "a", "d"]), data.args()
                .map(ToString::to_string)
                .collect::<Vec<_>>());
            let cases = data.cases().collect::<Vec<_>>();
            assert_eq!(2, cases.len());
            assert_eq!(to_args!(["42", "A{}", "D{}"]), cases[0].args());
            assert_eq!(to_args!(["43", "A{}", "D{}"]), cases[1].args());
            let fixtures = data.fixtures().cloned().collect::<Vec<_>>();
            assert_eq!(vec![fixture("the_fixture", vec!["42"])], fixtures);
        }

        #[test]
        fn case_could_be_arg_name() {
            let data = parse_data(r#"
                case,
                case(42)
                "#);

            assert_eq!("case", &data.args().next().unwrap().to_string());
            let cases = data.cases().collect::<Vec<_>>();
            assert_eq!(1, cases.len());
            assert_eq!(to_args!(["42"]), cases[0].args());
        }
    }

    mod parse_values_list {
        use super::*;
        use super::assert_eq;

        fn parse_values_list<S: AsRef<str>>(values_list: S) -> ValueList {
            parse_meta(values_list)
        }

        #[test]
        fn some_literals() {
            let literals = literal_expressions_str();
            let name = "argument";

            let values_list = parse_values_list(
                format!(r#"{} => [{}]"#, name,
                        literals
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<String>>()
                            .join(", "))
            );

            assert_eq!(name, &values_list.arg.to_string());
            assert_eq!(values_list.args(), to_args!(literals));
        }

        #[test]
        fn raw_code() {
            let values_list = parse_values_list(r#"no_mater => [vec![1,2,3]]"#);

            assert_eq!(values_list.args(), to_args!(["vec![1, 2, 3]"]));
        }

        #[test]
        #[should_panic(expected = r#"Cannot parse due"#)]
        fn raw_code_with_parsing_error() {
            parse_values_list(r#"other => [some:<>(1,2,3)]"#);
        }

        #[test]
        #[should_panic(expected = r#"expected square brackets"#)]
        fn forget_brackets() {
            parse_values_list(r#"other => 42"#);
        }
    }

    mod parse_matrix_info {
        use super::*;
        use super::assert_eq;
        use itertools::Itertools;

        fn parse_matrix_info<S: AsRef<str>>(matrix_info: S) -> MatrixInfo {
            parse_meta(matrix_info)
        }

        #[test]
        fn happy_path() {
            let info = parse_matrix_info(r#"
                expected => [12, 34 * 2],
                input => [format!("aa_{}", 2), "other"],
            "#);

            let value_ranges = info.args.list_values().collect_vec();
            assert_eq!(2, value_ranges.len());
            assert_eq!(to_args!(["12", "34 * 2"]), value_ranges[0].args());
            assert_eq!(to_args!([r#"format!("aa_{}", 2)"#, r#""other""#]), value_ranges[1].args());
            assert_eq!(info.modifiers, Default::default());
        }

        #[test]
        fn should_parse_modifiers_too() {
            let info = parse_matrix_info(r#"
                a => [12, 24, 42]
                ::trace
            "#);

            assert_eq!(Modifiers { modifiers: vec![RsTestAttribute::attr("trace")] },
                       info.modifiers);
        }

        #[test]
        fn should_parse_injected_fixtures_too() {
            let info = parse_matrix_info(r#"
                a => [12, 24, 42],
                fixture_1(42, "foo"),
                fixture_2("bar")
            "#);

            let fixtures = info.args.fixtures().cloned().collect_vec();

            assert_eq!(vec![fixture("fixture_1", vec!["42", r#""foo""#]),
                            fixture("fixture_2", vec![r#""bar""#])],
                       fixtures);
        }

        #[test]
        #[should_panic(expected = "should not be empty")]
        fn should_not_compile_if_empty_expression_slice() {
            parse_matrix_info(r#"
                invalid => []
            "#);
        }
    }
}

