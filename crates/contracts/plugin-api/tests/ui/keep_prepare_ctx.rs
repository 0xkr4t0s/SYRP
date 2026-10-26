// SPDX-License-Identifier: MIT OR Apache-2.0
// B1: a `PrepareCtx` is borrowed for one `prepare` call, so it cannot be kept for `process`.
use syrp_audio_api::ProcessConfig;
use syrp_plugin_api::{Params, PluginError, PrepareCtx, ProcessIo, Processor, RtCtx};

struct P(Option<&'static mut PrepareCtx<'static>>);
// SAFETY: never constructed; this only sets aside `Send` so the lifetime error stands alone.
unsafe impl Send for P {}
impl Processor for P {
    fn prepare(&mut self, _: &ProcessConfig, cx: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
        self.0 = Some(cx);
        Ok(())
    }
    fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {
        let _ = self.0.as_ref().map(|cx| cx.param_index("gain"));
    }
}

fn main() {}
