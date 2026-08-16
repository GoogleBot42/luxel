import Flash from "./Flash.svelte";
import "../app.css";

const target = document.getElementById("app");
if (!target) throw new Error("missing #app");

export default new Flash({ target });
