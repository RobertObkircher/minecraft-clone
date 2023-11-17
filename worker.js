import init, {process_message} from "./pkg/minecraft_clone.js";

console.log("Loaded wasm worker");

onmessage = async ev =>{
    console.log("Got event", ev)
    await init();
    // process_message(ev.data);

    greet();
}
function greet() {
    postMessage("world");
    setTimeout(greet, 5000);
}