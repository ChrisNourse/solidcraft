//! A weld's settings may save `tolerances`: the user's own silent band per kind of deviation
//! (`gap`, `overlap`, `tilt`, `drift`, `toe`, `misalignment`), in mm or degrees, in place of the
//! weld standard's value. It is left out of the file while empty, so older designs have none and
//! read as before; nothing is rewritten. The last release opens such a design with a warning about
//! the newer format number and keeps what it cannot read (`crates/doc/src/newer_file.rs`).
