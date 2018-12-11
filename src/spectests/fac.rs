// Rust test file autogenerated with cargo build (build/spectests.rs).
// Please do NOT modify it by hand, as it will be reseted on next build.
// Test based on spectests/fac.wast
#![allow(
    warnings,
    dead_code
)]
use wabt::wat2wasm;

use crate::webassembly::{instantiate, compile, ImportObject, ResultObject, Instance, Export};
use super::_common::{
    spectest_importobject,
    NaNCheck,
};


// Line 1
fn create_module_1() -> ResultObject {
    let module_str = "(module
      (type (;0;) (func (param i64) (result i64)))
      (func (;0;) (type 0) (param i64) (result i64)
        get_local 0
        i64.const 0
        i64.eq
        if (result i64)  ;; label = @1
          i64.const 1
        else
          get_local 0
          get_local 0
          i64.const 1
          i64.sub
          call 0
          i64.mul
        end)
      (func (;1;) (type 0) (param i64) (result i64)
        get_local 0
        i64.const 0
        i64.eq
        if (result i64)  ;; label = @1
          i64.const 1
        else
          get_local 0
          get_local 0
          i64.const 1
          i64.sub
          call 1
          i64.mul
        end)
      (func (;2;) (type 0) (param i64) (result i64)
        (local i64 i64)
        get_local 0
        set_local 1
        i64.const 1
        set_local 2
        block  ;; label = @1
          loop  ;; label = @2
            get_local 1
            i64.const 0
            i64.eq
            if  ;; label = @3
              br 2 (;@1;)
            else
              get_local 1
              get_local 2
              i64.mul
              set_local 2
              get_local 1
              i64.const 1
              i64.sub
              set_local 1
            end
            br 0 (;@2;)
          end
        end
        get_local 2)
      (func (;3;) (type 0) (param i64) (result i64)
        (local i64 i64)
        get_local 0
        set_local 1
        i64.const 1
        set_local 2
        block  ;; label = @1
          loop  ;; label = @2
            get_local 1
            i64.const 0
            i64.eq
            if  ;; label = @3
              br 2 (;@1;)
            else
              get_local 1
              get_local 2
              i64.mul
              set_local 2
              get_local 1
              i64.const 1
              i64.sub
              set_local 1
            end
            br 0 (;@2;)
          end
        end
        get_local 2)
      (func (;4;) (type 0) (param i64) (result i64)
        (local i64)
        i64.const 1
        set_local 1
        block  ;; label = @1
          get_local 0
          i64.const 2
          i64.lt_s
          br_if 0 (;@1;)
          loop  ;; label = @2
            get_local 1
            get_local 0
            i64.mul
            set_local 1
            get_local 0
            i64.const -1
            i64.add
            set_local 0
            get_local 0
            i64.const 1
            i64.gt_s
            br_if 0 (;@2;)
          end
        end
        get_local 1)
      (export \"fac-rec\" (func 0))
      (export \"fac-rec-named\" (func 1))
      (export \"fac-iter\" (func 2))
      (export \"fac-iter-named\" (func 3))
      (export \"fac-opt\" (func 4)))
    ";
    let wasm_binary = wat2wasm(module_str.as_bytes()).expect("WAST not valid or malformed");
    instantiate(wasm_binary, spectest_importobject(), None).expect("WASM can't be instantiated")
}

fn start_module_1(result_object: &ResultObject) {
    result_object.instance.start();
}

// Line 84
fn c1_l84_action_invoke(result_object: &ResultObject) {
    println!("Executing function {}", "c1_l84_action_invoke");
    let func_index = match result_object.module.info.exports.get("fac-rec") {
        Some(&Export::Function(index)) => index,
        _ => panic!("Function not found"),
    };
    let invoke_fn: fn(i64, &Instance) -> i64 = get_instance_function!(result_object.instance, func_index);
    let result = invoke_fn(25 as i64, &result_object.instance);
    assert_eq!(result, 7034535277573963776 as i64);
}

// Line 85
fn c2_l85_action_invoke(result_object: &ResultObject) {
    println!("Executing function {}", "c2_l85_action_invoke");
    let func_index = match result_object.module.info.exports.get("fac-iter") {
        Some(&Export::Function(index)) => index,
        _ => panic!("Function not found"),
    };
    let invoke_fn: fn(i64, &Instance) -> i64 = get_instance_function!(result_object.instance, func_index);
    let result = invoke_fn(25 as i64, &result_object.instance);
    assert_eq!(result, 7034535277573963776 as i64);
}

// Line 86
fn c3_l86_action_invoke(result_object: &ResultObject) {
    println!("Executing function {}", "c3_l86_action_invoke");
    let func_index = match result_object.module.info.exports.get("fac-rec-named") {
        Some(&Export::Function(index)) => index,
        _ => panic!("Function not found"),
    };
    let invoke_fn: fn(i64, &Instance) -> i64 = get_instance_function!(result_object.instance, func_index);
    let result = invoke_fn(25 as i64, &result_object.instance);
    assert_eq!(result, 7034535277573963776 as i64);
}

// Line 87
fn c4_l87_action_invoke(result_object: &ResultObject) {
    println!("Executing function {}", "c4_l87_action_invoke");
    let func_index = match result_object.module.info.exports.get("fac-iter-named") {
        Some(&Export::Function(index)) => index,
        _ => panic!("Function not found"),
    };
    let invoke_fn: fn(i64, &Instance) -> i64 = get_instance_function!(result_object.instance, func_index);
    let result = invoke_fn(25 as i64, &result_object.instance);
    assert_eq!(result, 7034535277573963776 as i64);
}

// Line 88
fn c5_l88_action_invoke(result_object: &ResultObject) {
    println!("Executing function {}", "c5_l88_action_invoke");
    let func_index = match result_object.module.info.exports.get("fac-opt") {
        Some(&Export::Function(index)) => index,
        _ => panic!("Function not found"),
    };
    let invoke_fn: fn(i64, &Instance) -> i64 = get_instance_function!(result_object.instance, func_index);
    let result = invoke_fn(25 as i64, &result_object.instance);
    assert_eq!(result, 7034535277573963776 as i64);
}

// Line 89

#[test]
fn test_module_1() {
    let result_object = create_module_1();
    // We group the calls together
    start_module_1(&result_object);
    c1_l84_action_invoke(&result_object);
    c2_l85_action_invoke(&result_object);
    c3_l86_action_invoke(&result_object);
    c4_l87_action_invoke(&result_object);
    c5_l88_action_invoke(&result_object);
}
