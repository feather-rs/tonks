extern crate proc_macro;

#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;

use syn::{FnArg, ItemFn, Pat, Type, DeriveInput};

#[proc_macro_derive(Resource)]
pub fn derive_resource(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: DeriveInput = parse_macro_input!(input as DeriveInput);

    let ident = &input.ident;

    let result = quote! {
        impl tonks::MacroData for &'static #ident {
            type SystemData = tonks::Read<#ident>;
        }

        impl tonks::MacroData for &'static mut #ident {
            type SystemData = tonks::Write<#ident>;
        }
    };

    result.into()
}

#[proc_macro_attribute]
pub fn system(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input: ItemFn = parse_macro_input!(input as ItemFn);

    let sig = &input.sig;
    assert!(
        sig.generics.params.is_empty(),
        "systems may not have generic parameters"
    );

    // Vector of system data variable names (`Ident`s).
    let mut resource_idents = vec![];
    // Vector of system data output types (`Type`s).
    let mut resource_types = vec![];

    // Find resource accesses.
    for arg in &sig.inputs {
        let pat_ty = match arg {
            FnArg::Typed(ty) => ty,
            _ => panic!("system cannot take `self` parameter"),
        };

        let ident = match &*pat_ty.pat {
            Pat::Ident(ident) => ident.ident.clone(),
            _ => panic!("parameter pattern not an ident"),
        };

        // Convert references to `Read<T>`/`Write<T>`
        let ty = match &*pat_ty.ty {
            Type::Reference(r) => {
                let ty = &*r.elem;
                let mutability = &r.mutability;

                quote! {
                    <&'static #mutability #ty as tonks::MacroData>::SystemData
                }
            },
            _ty => panic!("only references may be passed to systems"),
        };

        resource_idents.push(ident);
        resource_types.push(ty);
    }

    let block = &*input.block;
    let ident = &sig.ident;

    let res = quote! {
        #[allow(non_camel_case_types)]
        struct #ident;

        impl tonks::System for #ident {
            type SystemData = (#(#resource_types ,)*);

            fn run(&mut self, (#(#resource_idents ,)*): <Self::SystemData as tonks::SystemData>::Output) {
                #block
            }
        }
    };
    res.into()
}
