# checkle: Extremely fast checksum runner for arbitrarily large batches of large files

[![Open Source Starter Files](https://github.com/nrminor/checkle/actions/workflows/open-source-starter.yml/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/open-source-starter.yml) [![Rust CI](https://github.com/nrminor/checkle/actions/workflows/rust-ci.yml/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/rust-ci.yml)

A `checksum` utility for the multicore age. It's (going to be) so fast it will make you chuckle.

### Overview

I work in genomics. This means I often transfer small handfuls of files from sequencing cores, where each file can be as much as a half-a-terabyte. As such, checking the integrity of these files post-transfer can be an arduous, time-consuming task. In my experience, bioinformaticians tackle this problem with shell or Python for loops that will run `checksum` or some other single-threaded utility—and wait as long as days for the integrity checks to finish before they get going with their analyses.

`checkle` aims to make this approach obsolete. It will perform checksums on large batches of exceptionally large files transferred over the interwebs, using [Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree) to accelerate hashing on multicore machines.

### Development Goals

I have the following goals for `checkle`:

1. Find all recently transferred files based on a set of file attribute filters.
2. Spread hashing across as many (virtual) cores as possible using [Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree) (for the heads: `checkle` is a portmanteau of checksum and Merkle).
3. If a manifest of hashes from the source server is provided, spread post-transfer checksums across cores as well.
4. Support md5 for backward compatibility along with a few cryptographically secure hashing functions.
5. Be capable of reaching into `tar` and `zip` archives to checksum files without decompressing the whole archive.
6. Minimize filesystem calls and memory allocations to maximize performance.
7. Have an easy-to-use command line interface powered by [`clap`]().
8. Be easy to install, either through [crates.io](https://crates.io/) or with binaries for your platform of choice distributed in this repo.
9. Print a report to `stdout` on which files should be re-transferred.
10. Accept `stdin` to fit in with unix pipeline.

`checkle` will be made available on [crates.io](https://crates.io/) when it reaches a reasonable level of stability.
