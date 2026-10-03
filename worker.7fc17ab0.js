import init, { add_cover } from "./pkg/apng.0b585fe6.js";

const ready = init().then(() => {
    self.postMessage({ type: "ready" });
});

self.onmessage = async (e) => {
    const { id, input } = e.data;
    await ready; // 等 WASM 初始化完成

    try {
        const bytes = new Uint8Array(input);
        const t0 = performance.now();
        const out = add_cover(bytes);
        const ms = performance.now() - t0;

        // 把结果 buffer 以 transferable 方式送回，零拷贝
        self.postMessage({ type: "done", id, output: out.buffer, ms }, [
            out.buffer,
        ]);
    } catch (err) {
        self.postMessage({ type: "error", id, message: String(err) });
    }
};
