;; Core module for wasi_qa_component.wasm. Build: wasm-tools component embed <wit> core.wasm --world w | wasm-tools component new
;; where the WIT world imports wasi:io/poll@0.2.0 + wasi:clocks/monotonic-clock@0.2.0 and exports greentic:component/component-qa@0.6.0.
(module
  (import "wasi:clocks/monotonic-clock@0.2.0" "subscribe-duration" (func $sub (param i64) (result i32)))
  (import "wasi:io/poll@0.2.0" "[method]pollable.block" (func $block (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "\a1\a2\a3")
  (func (export "greentic:component/component-qa@0.6.0#qa-spec") (param i32) (result i32)
    ;; blocking WASI call: the sync host shim drives async I/O via block_on
    (call $block (call $sub (i64.const 1)))
    (i32.store (i32.const 0) (i32.const 16))
    (i32.store (i32.const 4) (i32.const 3))
    (i32.const 0))
  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) (i32.const 1024))
)
