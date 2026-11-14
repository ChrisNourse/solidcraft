//! A standard part (`parts`, by component id) can save how it was asked for beyond its family,
//! size and length: `thread` (`"modeled"` or `"none"`), `drive` (`"hex"`, `"hexalobular"`,
//! `"cross"`, `"slot"`), `pitch` (a fine pitch in mm), `left_hand` and `point` (a set
//! screw's `"cone"` or `"cup"`), so Change Part Size rebuilds the same part. Each is left out at
//! its default, and an older design has none of them: its parts have a cosmetic right-hand
//! coarse thread, their family's first drive and a flat point, as before. The last release warns about the newer
//! format number; the part's features are ordinary ones it reads as they are, and it does not
//! know the new keys, so resizing the part there rebuilds it with the defaults.
