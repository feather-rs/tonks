extern crate proc_macro;

#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;

use syn::{FnArg, ItemFn, Pat, Type, DeriveInput, Ident};
use proc_macro2::{TokenStream};

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

    let visibility = input.vis;

    let sig = &input.sig;
    assert!(
        sig.generics.params.is_empty(),
        "systems may not have generic parameters"
    );

    let (resource_idents, resource_types) = find_resource_accesses(&sig.inputs);

    let block = &*input.block;
    let ident = &sig.ident;
    let name = ident.to_string();

    let register = if cfg!(feature = "system-registry") {
        Some(quote! {
            tonks::inventory::submit!(tonks::SystemRegistration(tonks::parking_lot::Mutex::new(Some(Box::new(tonks::CachedSystem::new(#ident, #name))))));
        })
    } else {
        None
    };

    let res = quote! {
        #[allow(non_camel_case_types)]
        #visibility struct #ident;

        impl tonks::System for #ident {
            type SystemData = (#(#resource_types ,)*);

            fn run(&mut self, (#(#resource_idents ,)*): <Self::SystemData as tonks::SystemData>::Output) {
                #block
            }
        }

        #register
    };
    res.into()
}

#[proc_macro_attribute]
pub fn event_handler(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input: ItemFn = parse_macro_input!(input as ItemFn);

    let visibility = input.vis;

    let sig = &input.sig;
    assert!(
        sig.generics.params.is_empty(),
        "systems may not have generic parameters"
    );

    let ident = sig.ident.clone();

    // Find whether this is a batch handler or not, based on the first argument, which
    // is the event argument.
    let event_arg = sig.inputs.first().expect("event handler must take event as its first parameter");
    let event_ty = match event_arg {
        FnArg::Typed(p) => p,
        _ => panic!("event handler may not take self parameter"),
    };
    let event_ident = match &*event_ty.pat {
        Pat::Ident(ident) => ident.ident.clone(),
        _ => panic!("parameter pattern not an ident"),
    };

    let (is_batch, event_ty) = match &*event_ty.ty {
        Type::Reference(r) => {
            match *r.elem.clone() {
                Type::Slice(s) => (true, (&*s.elem).clone()),
                t => (false, t),
            }
        }
        _ => unimplemented!(),
    };

    let name = ident.to_string();

    let (resource_idents, resource_types) = find_resource_accesses(sig.inputs.iter().skip(1)); // Skip first argument

    let register = if cfg!(feature = "system-registry") {
        Some(quote! {
            tonks::inventory::submit!(tonks::HandlerRegistration(tonks::parking_lot::Mutex::new(Some(Box::new(tonks::CachedEventHandler::new(#ident, #name))))));
        })
    } else {
        None
    };

    let block = input.block.clone();

    let handle_impl = if is_batch {
        quote! {
            fn handle(&mut self, #event_ident: &#event_ty, (#(#resource_idents ,)*): &mut <Self::HandlerData as tonks::SystemData>::Output) {
                self.handle_batch(std::slice::from_ref(#event_ident), (#(#resource_idents ,)*))
            }
        }
    } else {
        quote! {
            fn handle(&mut self, #event_ident: &#event_ty, (#(#resource_idents ,)*): &mut <Self::HandlerData as tonks::SystemData>::Output) {
                #block
            }
        }
    };

    let handle_batch_impl = if is_batch {
        Some(quote! {
            fn handle_batch(&mut self, #event_ident: &[#event_ty], (#(#resource_idents ,)*): <Self::HandlerData as tonks::SystemData>::Output) {
                #block
            }
        })
    } else {
        None
    };

    let res = quote! {
        #[allow(non_camel_case_types)]
        #visibility struct #ident;

        impl tonks::EventHandler<#event_ty> for #ident {
            type HandlerData = (#(#resource_types ,)*);

            #handle_impl

            #handle_batch_impl
        }

        #register
    };

    res.into()
}

fn find_resource_accesses<'a>(inputs: impl IntoIterator<Item=&'a FnArg>) -> (Vec<Ident>, Vec<TokenStream>) {
    let mut resource_idents = vec![];
    let mut resource_types = vec![];

    for arg in inputs.into_iter() {
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

    (resource_idents, resource_types)
}
