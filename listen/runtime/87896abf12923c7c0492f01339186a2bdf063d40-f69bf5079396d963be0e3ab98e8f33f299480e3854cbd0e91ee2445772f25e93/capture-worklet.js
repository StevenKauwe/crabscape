// Capture only. Speech detection and recognition execute in the Rust worker.
// AudioContext is requested at 16 kHz, so the browser supplies its resampler.
class Capture extends AudioWorkletProcessor {
  constructor() { super(); this.frame = new Float32Array(512); this.used = 0;
    this.port.onmessage = ({data}) => { if (data === 'flush') {
      if(this.used) { this.port.postMessage({samples:this.frame},[this.frame.buffer]); this.frame=new Float32Array(512);this.used=0; }
      this.port.postMessage({flushed:true});
    }};
  }
  process(inputs) {
    const audio = inputs[0]?.[0]; if(!audio) return true;
    for(const value of audio) {
      this.frame[this.used++]=value;
      if(this.used===512) { this.port.postMessage({samples:this.frame},[this.frame.buffer]); this.frame=new Float32Array(512);this.used=0; }
    }
    return true;
  }
}
registerProcessor('speechworks-capture',Capture);
