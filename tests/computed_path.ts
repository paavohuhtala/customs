
const a = "a"
const b = "b"
const c = "c"

const obj = { a: { b: { c: "hello" } } }

export const hello = obj[a][b][c]

