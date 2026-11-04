# GY-RPI Level 2 in the runtime

`goya-runtime` keeps learned anticipation disabled by default. To opt in, a
host must provide a valid, scoped `ConsentRecord` and pass a monotonic time
value from the same clock domain used to set `expiry_ns`:

```rust,ignore
use goya_rpi2::ConsentRecord;
use goya_runtime::{Event, Runtime};
use goya_ir::ActionId;

// `runtime` is a constructed goya_runtime::Runtime.
runtime.opt_in_level2(
    ConsentRecord::granted("interaction-sequences", consent_expiry_ns),
    1024,
);
runtime.handle_at(Event::Activate(ActionId(1)), monotonic_now_ns);
if let Some(prediction) = runtime.predict_level2_at(monotonic_now_ns) {
    // Prepare optional work here; do not dispatch the predicted action.
    if runtime.level2_prediction_is_current(&prediction, monotonic_now_ns) {
        prepare_optional_work(prediction.action, prediction.probability);
    }
}
```

Only `handle_at` contributes eligible, declared button actions to training.
Predictions are returned only while opt-in and consent remain valid. Every
prediction carries the committed state revision and geometry generation that
produced it; the runtime rechecks both before speculative use. A state/layout
change makes the prediction stale. `opt_out_level2` withdraws consent and
erases retained transitions. `handle` remains the non-learning event path.

Level 2 predicts a likely successor; it never invokes that action or commits a
speculative result. Hosts may use the prediction to prepare cacheable work,
then recompute or discard it if the revision check fails.
