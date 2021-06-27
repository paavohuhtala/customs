# customs

A fast and lightweight<sup>1</sup> CLI tool for finding dead code & extraneous dependencies from TypeScript applications, written in Rust. It accomplishes its goal by parsing TypeScript files into an AST (using [swc](https://github.com/swc-project/swc)), performing name resolution & usage analysis and constructing a dependency graph from all exports and imports.

See also [glossary.md](./docs/glossary.md).

<sup>1</sup> I mean it, and not in the usual JavaScript way! See benchmarks below.

## Goals, non-goals and limitations

`customs` is designed for finding unused code in modern (post ES6) TypeScript applications.

- Code that doesn't use ES6 modules is not supported and will not supported.
- Customs is specifically designed for _applications_. It doesn't really make sense to check for unused exports in library projects, as usually most exports are going to be unused when no code is using the library.
- Right now customs only supports TypeScript, though nothing fundamentally prevents it from working with JavaScript.
- Since `customs` is not based on the TypeScript compiler (nor implements one <sub>[for now]</sub>), it can't validate that code is valid beyond syntax analysis. Code is assumed to be correct and have zero warnings under `strict: true`. If the code is somehow invalid, the output of the tool is undefined.
- The tool is not yet very mature, and only some parts of the application are comprehensively tested. Testing it with real-word codebases is likely to yield some interesting bugs.

## Where to get it?

Right now the only option is build it from source, but it is very easy. [Install a Rust toolchain](https://www.rust-lang.org/tools/install), clone the repo and then run `cargo build --release`.

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
    -a, --analyze <analyze>     [default: all]  [possible values: types, values, all]

ARGS:
    <target-dir>
```

## Comparison versus `ts-prune`

[`ts-prune`](https://github.com/nadeesha/ts-prune) is an excellent CLI tool with the same goal, and it was the primary inspiration for `customs`. It is written in TypeScript and it utilises the TypeScript compiler as a library for parsing and code analysis. There are some important differences between `ts-prune` and `customs`.

### Performance

`customs` is significantly faster than `ts-prune`. Here are some unscientific benchmark figures based on two real world codebases: a small Slack bot and a pretty large e-com frontend.

| sample name | number of modules | lines of code |
| ----------- | ----------------: | ------------: |
| bot         |                18 |            2K |
| ecom        |              1109 |          131K |

**Benchmarks: bot**

|       tool |    time | normalized time | peak memory | normalized peak memory |
| ---------: | ------: | :-------------- | :---------- | :--------------------- |
|  `customs` |   12 ms | 1x              | 1.2 MB      | 1x                     |
| `ts-prune` | 1158 ms | 97x             | 107 MB      | 89x                    |

**Benchmarks: ecom**

|       tool |    time | normalized time | peak memory | normalized peak memory |
| ---------: | ------: | :-------------- | :---------- | :--------------------- |
|  `customs` |  113 ms | 1x              | 6.8 MB      | 1x                     |
| `ts-prune` | 8600 ms | 76x             | 610 MB      | 90x                    |

These benchmarks were conducted on a desktop workstation with an 6-core (12 thread) Ryzen CPU and a top-of-the line NVME SSD.

### Why is `customs` so much faster?

- It doesn't implement _exactly_ the same feature set as `ts-prune`. While this is important to disclose to keep the benchmark fair, I believe reaching feature parity with `ts-prune` would not have a considerable effect on performance. I'll cover this in slightly more detail in the next section.
- It uses [swc](https://github.com/swc-project/swc)'s (dare I say) _blazing_ fast EcmaScript / TypeScript parser.
- It is written in Rust, and therefore benefits from optimized native code with no garbage collection.
- Since Rust has no runtime the program starts very quickly, which has a surprisingly large impact with smaller codebases.
  - Running `ts-prune` in an empty folder takes about 330 milliseconds, while doing the same with `customs` takes about 5 milliseconds.
  - This test was conducted on Windows 10, which is notorious for being relatively slow at spawning new processes.
- Reading and parsing source files is multithreaded using [Rayon](https://github.com/rayon-rs/rayon) by adding a single method call (`.par_bridge()`).
  - This gave about 5x speedup with 8 physical cores, which is great considering how little effort it took to implement.
- Some effort has been spent on thinking about algorithms and data structures to speed up analysis. In other words, I use `std`'s hashmaps and hash sets, and interned strings from [`string_cache`](https://crates.io/crates/string-cache).

### Missing (and added) features

`customs` almost but not quite matches `ts-prune`'s feature set and output. It is missing the following features:

- The tool doesn't yet handle wildcard imports correctly. All exports of a wildcard-imported module are marked as used (#12).
- It doesn't yet support dynamic imports (#10).
- It doesn't support annotating code to suppress warnings with magic comments.

But, it also has some additional features:

- It checks for unused NPM dependencies by parsing `package.json` and matching dependencies with import statements. It is quite limited at the moment, since it cannot find implicit dependencies added by a bundler (e.g `core-js`, `renegerator-runtime`) nor does it understand CSS packages (e.g `normalize.css`).
- It allows ignoring specified files and folders with a `.customsignore` file.

## License

Licensed under the MIT license.
