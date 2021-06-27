
const obj = {
  a: {
    b: {
      c: {
        d: 10
      }
    }
  }
}

const a = obj.a.b.c.d
export type A = typeof obj.a.b.c.d
