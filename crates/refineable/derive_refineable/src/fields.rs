use quote::format_ident;
use syn::{Field, PredicateType, TraitBound, Type, TypeParamBound, WherePredicate, parse_quote};

pub(crate) fn is_refineable_field(f: &Field) -> bool {
    f.attrs
        .iter()
        .any(|attr| attr.path().is_ident("refineable"))
}

pub(crate) fn is_optional_field(f: &Field) -> bool {
    if let Type::Path(typepath) = &f.ty
        && typepath.qself.is_none()
    {
        let segments = &typepath.path.segments;
        if segments.len() == 1 && segments.iter().any(|s| s.ident == "Option") {
            return true;
        }
    }
    false
}

pub(crate) fn get_wrapper_type(field: &Field, ty: &Type) -> syn::Type {
    if is_refineable_field(field) {
        let struct_name = if let Type::Path(tp) = ty {
            tp.path.segments.last().unwrap().ident.clone()
        } else {
            panic!("Expected struct type for a refineable field");
        };

        let refinement_struct_name = if struct_name.to_string().ends_with("Refinement") {
            format_ident!("{}", struct_name)
        } else {
            format_ident!("{}Refinement", struct_name)
        };
        let generics = if let Type::Path(tp) = ty {
            &tp.path.segments.last().unwrap().arguments
        } else {
            &syn::PathArguments::None
        };
        parse_quote!(#refinement_struct_name #generics)
    } else if is_optional_field(field) {
        ty.clone()
    } else {
        parse_quote!(Option<#ty>)
    }
}

pub(crate) fn wrapper_clone_bounds(wrapped_types: &[syn::Type]) -> Vec<WherePredicate> {
    wrapped_types
        .iter()
        .map(|ty| {
            WherePredicate::Type(PredicateType {
                lifetimes: None,
                bounded_ty: ty.clone(),
                colon_token: Default::default(),
                bounds: {
                    let mut punctuated = syn::punctuated::Punctuated::new();
                    punctuated.push_value(TypeParamBound::Trait(TraitBound {
                        paren_token: None,
                        modifier: syn::TraitBoundModifier::None,
                        lifetimes: None,
                        path: parse_quote!(Clone),
                    }));

                    punctuated
                },
            })
        })
        .collect()
}
