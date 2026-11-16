/* tslint:disable */
/* eslint-disable */

export class DecodeJob {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly audioSamples: number;
    readonly isFinal: boolean;
    readonly phrase: number;
    readonly samples: Float32Array;
}

export class DictationSession {
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    finish(): void;
    isCurrent(phrase: number): boolean;
    constructor(config: any);
    nextFinal(): DecodeJob | undefined;
    preview(): DecodeJob | undefined;
    pushInput(samples: Float32Array, rate: number): void;
    push(samples: Float32Array): void;
    /**
     * Speech probability from the most recently processed 32 ms frame.
     */
    speechProbability(): number;
}

export class Recognizer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    device(): string;
    fallbackReason(): string | undefined;
    resetStream(): void;
    /**
     * A growing, possibly revised phrase. Legacy models retain their existing
     * padded-preview behavior; streaming models consume only newly captured
     * audio, and append synthetic tail silence once at finalization.
     */
    transcribePhrase(samples: Float32Array, key: string, is_final: boolean, audio_samples: number): Promise<string>;
    transcribe(samples: Float32Array): Promise<string>;
}

export function loadModel(bytes: Uint8Array, backend: string): Promise<Recognizer>;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_decodejob_free: (a: number, b: number) => void;
    readonly __wbg_dictationsession_free: (a: number, b: number) => void;
    readonly __wbg_recognizer_free: (a: number, b: number) => void;
    readonly decodejob_audioSamples: (a: number) => number;
    readonly decodejob_isFinal: (a: number) => number;
    readonly decodejob_phrase: (a: number) => number;
    readonly decodejob_samples: (a: number) => [number, number];
    readonly dictationsession_cancel: (a: number) => void;
    readonly dictationsession_finish: (a: number) => [number, number];
    readonly dictationsession_isCurrent: (a: number, b: number) => number;
    readonly dictationsession_new: (a: any) => [number, number, number];
    readonly dictationsession_nextFinal: (a: number) => number;
    readonly dictationsession_preview: (a: number) => number;
    readonly dictationsession_push: (a: number, b: number, c: number) => [number, number];
    readonly dictationsession_pushInput: (a: number, b: number, c: number, d: number) => [number, number];
    readonly dictationsession_speechProbability: (a: number) => number;
    readonly loadModel: (a: number, b: number, c: number, d: number) => any;
    readonly recognizer_device: (a: number) => [number, number, number, number];
    readonly recognizer_fallbackReason: (a: number) => [number, number];
    readonly recognizer_resetStream: (a: number) => void;
    readonly recognizer_transcribe: (a: number, b: number, c: number) => any;
    readonly recognizer_transcribePhrase: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => any;
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___js_sys_7507379953139f4c___Function_fn_wasm_bindgen_d40562f4a4bf318b___JsValue_____wasm_bindgen_d40562f4a4bf318b___sys__Undefined___js_sys_7507379953139f4c___Function_fn_wasm_bindgen_d40562f4a4bf318b___JsValue_____wasm_bindgen_d40562f4a4bf318b___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wasm_bindgen_d40562f4a4bf318b___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_d40562f4a4bf318b___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wasm_bindgen_d40562f4a4bf318b___sys__JsNullable_wgpu_35b2085d9d71fc36___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_d40562f4a4bf318b___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wasm_bindgen_d40562f4a4bf318b___sys__JsNullable_wgpu_35b2085d9d71fc36___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_d40562f4a4bf318b___JsError___true__24: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wasm_bindgen_d40562f4a4bf318b___sys__JsNullable_wgpu_35b2085d9d71fc36___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_d40562f4a4bf318b___JsError___true__25: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wgpu_35b2085d9d71fc36___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true_: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen_d40562f4a4bf318b___convert__closures_____invoke___wgpu_35b2085d9d71fc36___backend__webgpu__webgpu_sys__gen_GpuDeviceLostInfo__GpuDeviceLostInfo______true__23: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
