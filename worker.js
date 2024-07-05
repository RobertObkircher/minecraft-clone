import init, {wasm_renderer, wasm_update, wasm_update_with_message} from "./pkg/minecraft_clone.js";

/** @type {Worker[]} */
let workers = [];
let initialized;

function do_update() {
    let timeout = wasm_update();
    if (timeout >= 0) {
        setTimeout(do_update, timeout);
    }
}

/**
 * @param {number} id
 * @param {MessageEvent} ev
 */
function do_update_with_message(id, ev) {
    let timeout = wasm_update_with_message(id, ev.data);
    if (timeout >= 0) {
        setTimeout(do_update, timeout);
    }
}

/**
 * @returns {number} thread id
 */
export function spawn_worker() {
    let worker = new Worker("worker.js", {type: "module"});
    let id = workers.push(worker);
    worker.onmessage = ev => {
        initialized.then(() => do_update_with_message(0, ev));
    };
    return id;
}

/**
 * @param {number} id
 * @param {Uint8Array} message
 */
export function post_message(id, message) {
    if (id === 0) {
        self.postMessage(message, [message.buffer]); // to parent
    } else {
        workers[id - 1].postMessage(message, [message.buffer]);
    }
}

// TODO find better way to do this
self.spawn_worker = spawn_worker;
self.post_message = post_message;

initialized = init();
if (self.document) {
    await initialized;
    let disable_webgpu = !navigator.gpu || await navigator.gpu.requestAdapter() === null;
    console.log("starting wasm_renderer(disable_webgpu=" + disable_webgpu + ")");
    await wasm_renderer(disable_webgpu);
} else {
    onmessage = ev => {
        initialized.then(() => do_update_with_message(0, ev));
    };
    await initialized;
}
