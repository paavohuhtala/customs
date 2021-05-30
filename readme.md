# customs
A CLI tool for finding unnecessary module exports from TypeScript projects, written in Rust using [swc](https://github.com/swc-project/swc).

## Where to get it?

Right now the only option is build it from source, but it is very easy. Install a Rust toolchain, clone the repo and then run `cargo build --release`.

## `--help`

```
customs 0.1
Paavo Huhtala <paavo.huhtala@gmail.com>

USAGE:
    customs.exe [OPTIONS] <target-dir>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -f, --format <format>     [default: compact]  [possible values: clean, compact]

ARGS:
    <target-dir>
```


## Comparison versus `ts-prune`

[`ts-prune`](https://github.com/nadeesha/ts-prune) is an excellent CLI tool targeting with the same goal, and it was the primary inspiration for `customs`. It is written in TypeScript and it utilises the TypeScript compiler as a library for parsing and code analysis. There are a number of important differences between `ts-prune` and `customs`.

### Performance

`customs` is significantly faster than `ts-prune`. Here are some unscientific benchmark figures based on two real world codebases: a small Slack bot and a pretty large e-com frontend. 


|number of modules|lines of code|`ts-prune`|`customs`|speedup          |
|----------------:|------------:|---------:|--------:|:----------------|
|18               |2K           |1072 ms   |5 ms     |214 times faster |
|1109             |125K         |7111 ms   |92 ms    |77 times faster  |

These benchmarks were conducted on a desktop workstation with an 8-core (16 thread) Ryzen CPU and a top-of-the line NVME SSD.

Why is `customs` so much faster? 

- It doesn't implement _exactly_ the same feature set as `ts-prune`. While this is important to disclose to keep the benchmark fair, I believe reaching feature parity with `ts-prune` would not have a considerabale effect on performance. I'll cover this in slightly more detail in the next section.
- It uses swc which has a really fast EcmaScript / TypeScript parser.
- It is written in Rust, and therefore benefits from native code and no garbage collection.
- Since Rust has no runtime the program starts very quickly, which has a large impact with smaller codebases.
  - Running `ts-prune` in an empty folder takes about 330 milliseconds, while doing the same with `customs` takes about 5 milliseconds.
  - This test was conducted on Windows 10, which is notorious for being relatively slow at spawning new processes.
- Reading and parsing source files is multithreaded using [Rayon](https://github.com/rayon-rs/rayon) by adding a single method call (`.par_bridge()`).
  - This gave about 5x speedup with 8 physical cores, which is great considering how little effort it took to implement.

### Missing features

Simply put, for now `customs` has less features. Perhaps most importantly it does not perform any analysis which would require semantic analysis to do properly. This includes:

- Analysis of which exports are completely unused and which ones are only used locally.
- Analysis of wildcard imports (`import * as foo from './foo'`).
  - Right now the tool (pessimistically) marks all exports as used if the module is wildcard imported at all.

`ts-prune` analyses these cases, but my understanding (which might be entirely wrong) is that it does it using a simple "does this module contain this identifier" check. This is good enough in many cases, but strictly speaking it is insufficient in a language where identifiers can [shadow](https://en.wikipedia.org/wiki/Variable_shadowing) each other, which TypeScript definitely is. (In fact, TypeScript is more complicated than JS because types and values live in mostly different namespaces: you can have a type and a variable of the same name in the same file, and export them separately. Not to mention classes which are both types and values at the same time.) I might implement these later, either using the simple identifier check or a more elaborate light semantic analysis pass, which would take block scoping and shadowing into account.

## ToDo

- Add some tests
- Setup a CI pipeline
- Report locally used exports separately vs completely dead code
- Support .gitignore
- Support JS files
- Handle non-code imports (e.g CSS modules) without an ugly error message
- Allow marking functions and modules as used with magic comments and/or config files
- Long tearm goals
  - More comprehensive dead code analysis. Figure out which functions and types are unused, and then see if removing them would lead to even more code being removed.
  - Find unused external dependencies using `package.json`


## License

Licensed under the MIT license.