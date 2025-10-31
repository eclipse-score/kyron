//
// Copyright (c) 2025 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
//

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{braced, bracketed, parse_macro_input, Expr, Ident, ItemFn, LitBool, LitStr, Token};

// MacroArgs holds the parsed attribute parameters.
struct MacroArgs {
    task_queue_size: Option<Expr>,
    worker_threads: Option<Expr>,
    worker_thread_parameters: Option<ThreadParams>,
    safety_worker: Option<bool>,
    safety_worker_thread_parameters: Option<ThreadParams>,
    dedicated_workers: Vec<DedicatedWorker>,
}

impl MacroArgs {
    fn new() -> Self {
        Self {
            task_queue_size: None,
            worker_threads: None,
            worker_thread_parameters: None,
            safety_worker: None,
            safety_worker_thread_parameters: None,
            dedicated_workers: Vec::new(),
        }
    }
}

// ThreadParams: { priority = 10, scheduler_type = "Fifo/RoundRobin/Other", affinity = [0,1], stack_size = 123 }
#[derive(Clone)]
struct ThreadParams {
    priority: Option<Expr>,
    scheduler_type: Option<LitStr>,
    affinity: Option<Vec<Expr>>,
    stack_size: Option<Expr>,
}

impl ThreadParams {
    fn new() -> Self {
        ThreadParams {
            priority: None,
            scheduler_type: None,
            affinity: None,
            stack_size: None,
        }
    }
}

// DedicatedWorker: { id = "dedicated1", thread_parameters = { ... } }
struct DedicatedWorker {
    id: LitStr,
    thread_parameters: Option<ThreadParams>,
}

// Parse top-level attribute list
impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = MacroArgs::new();

        while !input.is_empty() {
            // parse a key ident
            let lookahead = input.lookahead1();
            if lookahead.peek(Token![,]) {
                let _comma: Token![,] = input.parse()?;
                continue;
            }

            let key: Ident = input.parse()?;
            // expect '='
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "task_queue_size" => {
                    let expr: Expr = input.parse()?;
                    args.task_queue_size = Some(expr);
                }
                "worker_threads" => {
                    let expr: Expr = input.parse()?;
                    args.worker_threads = Some(expr);
                }
                "worker_thread_parameters" => {
                    let tp: ThreadParams = parse_braced_thread_params(&input)?;
                    args.worker_thread_parameters = Some(tp);
                }
                "safety_worker" => {
                    let b: LitBool = input.parse()?;
                    args.safety_worker = Some(b.value());
                }
                "safety_worker_thread_parameters" => {
                    let tp: ThreadParams = parse_braced_thread_params(&input)?;
                    args.safety_worker_thread_parameters = Some(tp);
                }
                "dedicated_workers" => {
                    let inner;
                    bracketed!(inner in input);
                    while !inner.is_empty() {
                        // the value is a braced block with id = "...", thread_parameters = { ... }
                        let content;
                        braced!(content in inner);

                        let mut id_opt: Option<LitStr> = None;
                        let mut thread_params_opt: Option<ThreadParams> = None;

                        while !content.is_empty() {
                            let key2: Ident = content.parse()?;
                            content.parse::<Token![=]>()?;
                            match key2.to_string().as_str() {
                                "id" => {
                                    let s: LitStr = content.parse()?;
                                    id_opt = Some(s);
                                }
                                "thread_parameters" => {
                                    let tp = parse_braced_thread_params(&&content)?;
                                    thread_params_opt = Some(tp);
                                }
                                other => {
                                    return Err(syn::Error::new_spanned(key2, format!("Unknown key in dedicated_worker: {}", other)));
                                }
                            }

                            if content.peek(Token![,]) {
                                let _c: Token![,] = content.parse()?;
                            }
                        }

                        let id = id_opt.ok_or_else(|| syn::Error::new_spanned(&key, "dedicated_worker missing required `id`"))?;

                        args.dedicated_workers.push(DedicatedWorker {
                            id,
                            thread_parameters: thread_params_opt,
                        });

                        if inner.peek(Token![,]) {
                            let _c: Token![,] = inner.parse()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new_spanned(key, format!("Unknown attribute key: {}", other)));
                }
            }

            // consume optional trailing comma
            if input.peek(Token![,]) {
                let _comma: Token![,] = input.parse()?;
            }
        }

        Ok(args)
    }
}

// Helper to parse `{ priority = 10, scheduler_type = "Fifo", affinity = [0,1], stack_size = 123 }`
fn parse_braced_thread_params(input: &ParseStream) -> Result<ThreadParams> {
    let content;
    braced!(content in *input);

    let mut tp = ThreadParams::new();

    while !content.is_empty() {
        let key: Ident = content.parse()?;
        content.parse::<Token![=]>()?;

        match key.to_string().as_str() {
            "priority" => {
                let v: Expr = content.parse()?;
                tp.priority = Some(v);
            }
            "scheduler_type" => {
                let s: LitStr = content.parse()?;
                tp.scheduler_type = Some(s);
            }
            "affinity" => {
                // parse bracketed list [0,1]
                let inner;
                bracketed!(inner in content);
                let mut vals: Vec<Expr> = Vec::new();
                while !inner.is_empty() {
                    let e: Expr = inner.parse()?;
                    vals.push(e);
                    if inner.peek(Token![,]) {
                        let _c: Token![,] = inner.parse()?;
                    }
                }
                tp.affinity = Some(vals);
            }
            "stack_size" => {
                let v: Expr = content.parse()?;
                tp.stack_size = Some(v);
            }
            other => {
                return Err(syn::Error::new_spanned(key, format!("Unknown key in thread parameters: {}", other)));
            }
        }

        if content.peek(Token![,]) {
            let _comma: Token![,] = content.parse()?;
        }
    }

    Ok(tp)
}

// Convert Expr to usize if it's a literal integer
fn expr_to_usize(expr: &Expr) -> Result<usize> {
    if let Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Int(ref lit_int),
        ..
    }) = expr
    {
        lit_int.base10_parse::<usize>()
    } else {
        Err(syn::Error::new_spanned(expr, "Expected integer literal."))
    }
}

///
/// The macro that allow user to declare `async` beautified main function what will
/// configure and run function body in runtime waiting for it to finish
///
/// # Usage
///```ignore
/// #[kyron::main(
///     task_queue_size = 128,                // Optional, must be power of two, default: 256
///     worker_threads = 4,                   // Optional, range: 1..=128, default: 2
///     worker_thread_parameters = {          // Optional, default: inherited from parent thread
///         priority = 10,                    // Optional, 0..=255
///         scheduler_type = "Fifo",          // Optional, "Fifo" | "RoundRobin" | "Other"
///         affinity = [0, 1],                // Optional, list of CPU ids
///         stack_size = 4096                 // Optional, bytes, default: OS specific
///     },
///     safety_worker = true,                 // Optional, enables safety worker
///     safety_worker_thread_parameters = {   // Optional, same keys as above
///         priority = 20,
///         scheduler_type = "RoundRobin"
///     },
///     dedicated_workers = [                 // Optional, list of dedicated workers
///         {
///             id = "dedicated1",            // Required, unique id
///             thread_parameters = {         // Optional, same keys as above
///                 priority = 30,
///                 scheduler_type = "Fifo"
///             }
///         },
///         {
///             id = "dedicated2"
///             // thread_parameters omitted, uses defaults
///         }
///     ]
/// )]
/// async fn main() {
///     // User async code here
/// }
///```
///
/// ## Notes:
/// - All parameters are optional unless otherwise noted.
/// - `task_queue_size` must be a power of two.
/// - `worker_threads` must be between 1 and 128.
/// - `priority` must be between 0 and 255.
/// - `scheduler_type` must be one of "Fifo", "RoundRobin", "Other".
/// - Dedicated worker `id`s must be unique.
/// - The function name must be `main`, be `async`, and take no arguments.
/// - If either `priority` or `scheduler_type` or both to be set in thread parameters to desired value,
///   then both should be set to avoid inheriting from parent thread.
///
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    // parse the attribute
    let args = parse_macro_input!(attr as MacroArgs);
    // parse the input function
    let input_fn = parse_macro_input!(item as ItemFn);

    // function name must be main
    if input_fn.sig.ident != "main" {
        return syn::Error::new_spanned(&input_fn.sig.ident, "Function name must be 'main'")
            .to_compile_error()
            .into();
    }

    // require async
    if input_fn.sig.asyncness.is_none() {
        return syn::Error::new_spanned(input_fn.sig.fn_token, "main() function must be async")
            .to_compile_error()
            .into();
    }

    // input args must be empty
    if !input_fn.sig.inputs.is_empty() {
        return syn::Error::new_spanned(&input_fn.sig.inputs, "main() function must not have arguments.")
            .to_compile_error()
            .into();
    }

    // original items preserved for the generated fn
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_ret = &input_fn.sig.output;

    // build tokens for worker_thread_parameters if present
    let worker_tp_tokens = match args.worker_thread_parameters {
        Some(tp) => {
            if tp.priority.is_none() ^ tp.scheduler_type.is_none() {
                eprintln!("*** Warning: Either priority or scheduler type is not configured for async worker, both attributes will be inherited from parent thread.");
            }
            thread_parameters_to_tokens(&tp, true)
        }
        None => quote! { /* no worker params */ },
    };

    // safety worker token if enabled
    let sw_enabled = args.safety_worker.unwrap_or(false);
    let safety_worker_tokens = if sw_enabled {
        match args.safety_worker_thread_parameters {
            Some(tp) => {
                if tp.priority.is_none() ^ tp.scheduler_type.is_none() {
                    eprintln!("*** Warning: Either priority or scheduler type is not configured for safety worker, both attributes will be inherited from parent thread.");
                }
                let tp_tokens = thread_parameters_to_tokens(&tp, false);
                quote! {
                    .enable_safety_worker(
                        ThreadParameters::new()
                        #tp_tokens
                    )
                }
            }
            None => quote! {
                .enable_safety_worker(ThreadParameters::default())
            },
        }
    } else {
        quote! {/* safety worker not enabled */}
    };

    // Check whether dedicated worker id is unique
    let mut ids = std::collections::HashSet::new();
    for dw in args.dedicated_workers.iter() {
        if !ids.insert(dw.id.value()) {
            return syn::Error::new_spanned(&dw.id, format!("Duplicate dedicated worker id: {}", dw.id.value()))
                .to_compile_error()
                .into();
        }
    }
    // dedicated workers tokens
    let mut dedicated_calls = Vec::new();
    for dw in args.dedicated_workers.iter() {
        let id = &dw.id;
        let dw_tp_tokens = if let Some(tp) = &dw.thread_parameters {
            if tp.priority.is_none() ^ tp.scheduler_type.is_none() {
                eprintln!("*** Warning: Either priority or scheduler type is not configured for worker {:?}, both attributes will be inherited from parent thread.", dw.id.value().as_str());
            }
            let tp_tokens = thread_parameters_to_tokens(tp, false);
            quote! {
                ThreadParameters::new()
                #tp_tokens
            }
        } else {
            quote! { ThreadParameters::default() }
        };
        dedicated_calls.push(quote! {
            .with_dedicated_worker(#id.into(), #dw_tp_tokens)
        });
    }

    // base numeric args: task_queue_size, worker_threads
    let task_queue_size_ts = match args.task_queue_size {
        Some(e) => {
            let queue_size = expr_to_usize(&e).unwrap();
            if !queue_size.is_power_of_two() {
                return syn::Error::new_spanned(e, "'task_queue_size' must be a power of two.")
                    .to_compile_error()
                    .into();
            }
            quote! { #e }
        }
        None => quote! { 256 }, // default
    };

    let worker_threads_ts = match args.worker_threads {
        Some(e) => {
            let cnt = expr_to_usize(&e).unwrap();
            if !(1..=128).contains(&cnt) {
                return syn::Error::new_spanned(e, "'worker_threads' must be between 1 and 128.")
                    .to_compile_error()
                    .into();
            }
            quote! { #e }
        }
        None => quote! { 2 }, // default
    };

    let return_clause = match fn_ret {
        syn::ReturnType::Default => quote!(), // no return type
        syn::ReturnType::Type(_, ret_type) => quote!(-> #ret_type),
    };

    // Now produce the expansion
    let expanded = quote! {
        use kyron::prelude::*;


        #fn_vis fn main() #return_clause{
            // Build runtime
            let (builder, _engine_id) = RuntimeBuilder::new().with_engine(
                ExecutionEngineBuilder::new()
                .task_queue_size(#task_queue_size_ts)
                .workers(#worker_threads_ts)
                #worker_tp_tokens
                #safety_worker_tokens
                #(#dedicated_calls)*
            );
            let mut runtime = builder.build().expect("Failed to build runtime.");

            runtime.block_on(async {
                // Original function body
                #fn_block
            })
        }
    };

    TokenStream::from(expanded)
}

// Convert ThreadParams struct into tokens
fn thread_parameters_to_tokens(tp: &ThreadParams, is_async_worker: bool) -> proc_macro2::TokenStream {
    let priority_token = if let Some(priority) = &tp.priority {
        let pri = expr_to_usize(priority).unwrap();
        if pri > 255 {
            return syn::Error::new_spanned(priority, "'priority' must be between 0 and 255.").to_compile_error();
        }
        if is_async_worker {
            quote! { .thread_priority(#priority) }
        } else {
            quote! { .priority(#priority) }
        }
    } else {
        quote! {}
    };

    let scheduler_type_token = if let Some(scheduler_type) = &tp.scheduler_type {
        let st = match scheduler_type.value().as_str() {
            "Fifo" => quote! { kyron::scheduler::SchedulerType::Fifo },
            "RoundRobin" => quote! { kyron::scheduler::SchedulerType::RoundRobin },
            "Other" => quote! { kyron::scheduler::SchedulerType::Other },
            other => {
                return syn::Error::new_spanned(scheduler_type, format!("Invalid scheduler_type: {}", other)).to_compile_error();
            }
        };
        if is_async_worker {
            quote! { .thread_scheduler(#st) }
        } else {
            quote! { .scheduler_type(#st) }
        }
    } else {
        quote! {}
    };

    let affinity_token = if let Some(affinity) = &tp.affinity {
        if is_async_worker {
            quote! { .thread_affinity(&[#(#affinity),*]) }
        } else {
            quote! { .affinity(&[#(#affinity),*]) }
        }
    } else {
        quote! {}
    };

    let stack_size_token = if let Some(stack_size) = &tp.stack_size {
        if is_async_worker {
            quote! { .thread_stack_size(#stack_size) }
        } else {
            quote! { .stack_size(#stack_size) }
        }
    } else {
        quote! {}
    };

    quote! {
        #priority_token
        #scheduler_type_token
        #affinity_token
        #stack_size_token
    }
}
