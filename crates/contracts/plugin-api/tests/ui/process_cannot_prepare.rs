// SPDX-License-Identifier: MIT OR Apache-2.0
// B1/B4: `process` cannot make a `PrepareCtx` (no public constructor or fields), and nothing it
// is given resolves parameter string ids.
use syrp_audio_api::ProcessConfig;
use syrp_plugin_api::{Params, PluginError, PrepareCtx, ProcessIo, Processor, RtCtx};

struct P;
impl Processor for P {
    fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
        Ok(())
    }
    fn process(&mut self, io: &mut ProcessIo<'_>, params: &Params, rt: &mut RtCtx<'_>) {
        let _cx = PrepareCtx::new();
        let _cx = PrepareCtx { param_change_capacity: 0 };
        let _ = params.index_of("gain");
        let _ = io.param_index("gain");
        let _ = rt.param_index("gain");
    }
}

fn main() {}
