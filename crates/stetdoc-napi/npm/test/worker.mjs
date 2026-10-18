// A worker thread of the smoke test: it loads the addon in its own thread, opens the package it is given, makes one tracked
// edit and posts back what it saw. A server that keeps documents between requests can run every call in a worker like this one.
import { parentPort, workerData } from "node:worker_threads";
import { Document, version } from "../index.js";

const doc = Document.open(workerData.bytes);
const receipt = doc.edit({
  base: doc.base(),
  ops: [{ op: "replace", ref: "p3", start: 0, end: 7, with: workerData.with }],
  author: "Worker",
  date: "2026-10-06T10:00:00Z",
});
const text = doc.view({ mode: "final" }).text;
parentPort.postMessage({ version, base: receipt.base, text, saved: doc.save().bytes.length });
