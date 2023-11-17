import init, {wasm_main, wasm_worker, wasm_onmessage } from "./pkg/minecraft_clone.js";

/** @type {Worker[]} */
let workers = [];

/**
 * @returns {number}
 */
export function hardware_concurrency() {
    return navigator.hardwareConcurrency;
}

/**
 * @returns {number} thread id
 */
export function spawn_worker() {
    let worker = new Worker("worker.js", {type: "module"});
    let id = workers.push(worker);
    worker.onmessage = ev => {
        wasm_onmessage(id, ev.data);
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
self.hardware_concurrency = hardware_concurrency;
self.spawn_worker = spawn_worker;
self.post_message = post_message;

await init();

if (self.document) {
    await wasm_main();
} else {
    onmessage = ev => {
        wasm_onmessage(0, ev.data);
    };
    wasm_worker();
    // function do_work() {
    //     let timeout = wasm_worker();
    //     setTimeout(do_work, timeout);
    // }
    // do_work();
}
