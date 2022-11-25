# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.4.0

- Added checksum module with CRC32Sink to calculate CRC32.
- Added counter module with ByteCounter and ByteLimit.
- Enable optional features generation on docs.rs.
- Update tokio to 1.22.0 
- Update anyhow to 1.0.66
- Update futures to 0.3.25

## 0.3.0

- Added a `try_finally` helper for running async cleanup code at the end of
  an async Result-returning block.

## 0.2.0

- Added the `try_apply_chunker()` function.

## 0.1.0

- Initial release of the `cobalt-async` crate.
- Added the `apply_chunker()` function.
