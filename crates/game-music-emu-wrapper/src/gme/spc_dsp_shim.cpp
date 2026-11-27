// Test-only C shim exposing Spc_Dsp directly, so the Rust port's DSP can be
// differentially tested in isolation from the SPC700 CPU.

#include "Spc_Dsp.h"

#include "Fir_Resampler.h"
#include <vector>

struct Spc_Dsp_Test_Access {
    static Spc_Dsp::state_t const& state(Spc_Dsp const* dsp) { return dsp->m; }
};

// Record DSP runs and register writes during differential tests.
struct DspWriteEvent {
	int time;
	int addr;
	int data;
};
// thread_local: tests run in parallel threads, each with its own emulator
static thread_local std::vector<DspWriteEvent> dsp_write_trace;
static thread_local bool trace_enabled = false;

void gme_dsp_write_hook_cxx(int time, int addr, int data)
{
	if (trace_enabled) dsp_write_trace.push_back(DspWriteEvent { time, addr, data });
}

extern "C" {

void spc_dsp_trace_reset() { dsp_write_trace.clear(); trace_enabled = true; }

int spc_dsp_trace_len() { return (int) dsp_write_trace.size(); }

void spc_dsp_trace_get(int i, int* time, int* addr, int* data)
{
	*time = dsp_write_trace[i].time;
	*addr = dsp_write_trace[i].addr;
	*data = dsp_write_trace[i].data;
}

Spc_Dsp* spc_dsp_shim_new() { return new Spc_Dsp(); }

void spc_dsp_shim_delete(Spc_Dsp* dsp) { delete dsp; }

void spc_dsp_shim_init(Spc_Dsp* dsp, void* ram_64k) { dsp->init(ram_64k); }

void spc_dsp_shim_load(Spc_Dsp* dsp, unsigned char const* regs) { dsp->load(regs); }

void spc_dsp_shim_set_output(Spc_Dsp* dsp, short* out, int out_size)
{
	dsp->set_output(out, out_size);
}

void spc_dsp_shim_run(Spc_Dsp* dsp, int clock_count) { dsp->run(clock_count); }

int spc_dsp_shim_sample_count(Spc_Dsp const* dsp) { return dsp->sample_count(); }

int spc_dsp_shim_read(Spc_Dsp const* dsp, int addr) { return dsp->read(addr); }

void spc_dsp_shim_write(Spc_Dsp* dsp, int addr, int data) { dsp->write(addr, data); }

void spc_dsp_shim_mute_voices(Spc_Dsp* dsp, int mask) { dsp->mute_voices(mask); }

// Voice state getters
int spc_dsp_shim_voice_env(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].env; }
int spc_dsp_shim_voice_env_mode(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].env_mode; }
int spc_dsp_shim_voice_kon_delay(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].kon_delay; }
int spc_dsp_shim_voice_interp_pos(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].interp_pos; }
int spc_dsp_shim_voice_brr_addr(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].brr_addr; }
int spc_dsp_shim_voice_brr_offset(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].brr_offset; }
int spc_dsp_shim_voice_hidden_env(Spc_Dsp const* dsp, int v) { return Spc_Dsp_Test_Access::state(dsp).voices[v].hidden_env; }
int spc_dsp_shim_voice_buf_pos(Spc_Dsp const* dsp, int v)
{
	return (int) (Spc_Dsp_Test_Access::state(dsp).voices[v].buf_pos - Spc_Dsp_Test_Access::state(dsp).voices[v].buf);
}

// ---- Fir_Resampler<24> shim (the resampler used by Spc_Emu) ----

Fir_Resampler<24>* fir24_shim_new() { return new Fir_Resampler<24>(); }
void fir24_shim_delete(Fir_Resampler<24>* r) { delete r; }
void fir24_shim_buffer_size(Fir_Resampler<24>* r, int size) { r->buffer_size(size); }
double fir24_shim_time_ratio(Fir_Resampler<24>* r, double factor, double rolloff, double gain)
{
	return r->time_ratio(factor, rolloff, gain);
}
void fir24_shim_clear(Fir_Resampler<24>* r) { r->clear(); }
int fir24_shim_max_write(Fir_Resampler<24> const* r) { return ((Fir_Resampler<24>*) r)->max_write(); }
short* fir24_shim_buffer(Fir_Resampler<24>* r) { return r->buffer(); }
void fir24_shim_write(Fir_Resampler<24>* r, long count) { r->write(count); }
int fir24_shim_read(Fir_Resampler<24>* r, short* out, int count) { return r->read(out, count); }
int fir24_shim_skip_input(Fir_Resampler<24>* r, long count) { return r->skip_input(count); }

// Global state getters
int spc_dsp_shim_kon(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).kon; }
int spc_dsp_shim_new_kon(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).new_kon; }
int spc_dsp_shim_t_koff(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).t_koff; }
int spc_dsp_shim_noise(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).noise; }
int spc_dsp_shim_every_other_sample(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).every_other_sample; }
int spc_dsp_shim_echo_offset(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).echo_offset; }
int spc_dsp_shim_phase(Spc_Dsp const* dsp) { return Spc_Dsp_Test_Access::state(dsp).phase; }
unsigned spc_dsp_shim_counter(Spc_Dsp const* dsp, int i) { return Spc_Dsp_Test_Access::state(dsp).counters[i]; }

} // extern "C"
