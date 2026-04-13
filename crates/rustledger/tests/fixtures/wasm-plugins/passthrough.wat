;; Minimal WASM plugin stub for testing dispatch.
;; Exports the required interface (memory, alloc, process) but the
;; process function returns (ptr=0, len=0), which causes a
;; deserialization error in the runtime. This is intentional — the
;; test verifies that the dispatch code is reached (error output),
;; not that the plugin produces valid results.
(module
  (memory (export "memory") 1)
  (func (export "alloc") (param i32) (result i32)
    i32.const 0)
  (func (export "process") (param i32 i32) (result i64)
    i64.const 0))
