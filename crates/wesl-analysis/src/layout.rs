//! WGSL memory layout: alignment, size and member offsets for host-shareable types.
//!
//! Implements the address-space layout rules from the WGSL specification, §"Memory Layout".
//! This is what struct layout inlay hints display — a uniform buffer whose Rust-side and
//! shader-side layouts disagree fails silently at runtime, and the padding that causes it is
//! invisible in the source.
//!
//! Types that are not host-shareable (`bool`, samplers, textures, pointers) and runtime-sized
//! arrays have no computable size here, so they yield `None` and produce no hint.

use std::collections::HashMap;

use crate::ty::Ty;

/// Per-member `@align`/`@size` overrides for a struct, keyed by struct name and stored in
/// member order. Threaded through so nested structs are measured with their own attributes
/// rather than their natural layout.
pub type MemberOverrides = HashMap<String, Vec<(Option<u32>, Option<u32>)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberLayout {
    pub offset: u32,
    pub align: u32,
    pub size: u32,
}

fn round_up(alignment: u32, value: u32) -> Option<u32> {
    if alignment == 0 {
        return Some(value);
    }
    value.div_ceil(alignment).checked_mul(alignment)
}

pub fn align_of(ty: &Ty, overrides: &MemberOverrides) -> Option<u32> {
    Some(match ty {
        Ty::I32 | Ty::U32 | Ty::F32 => 4,
        Ty::F16 => 2,
        Ty::Vector(2, element) => 2u32.checked_mul(align_of(element, overrides)?)?,
        Ty::Vector(3 | 4, element) => 4u32.checked_mul(align_of(element, overrides)?)?,
        Ty::Matrix(_, rows, element) => align_of(&Ty::Vector(*rows, element.clone()), overrides)?,
        Ty::Array(element, _) => align_of(element, overrides)?,
        Ty::Struct(name, fields) => struct_layout(name, fields, overrides)?
            .iter()
            .map(|member| member.align)
            .max()
            .unwrap_or(1),
        _ => return None,
    })
}

pub fn size_of(ty: &Ty, overrides: &MemberOverrides) -> Option<u32> {
    Some(match ty {
        Ty::I32 | Ty::U32 | Ty::F32 => 4,
        Ty::F16 => 2,
        Ty::Vector(size, element) => u32::from(*size).checked_mul(size_of(element, overrides)?)?,
        Ty::Matrix(columns, rows, element) => {
            let column = Ty::Vector(*rows, element.clone());
            let element_size =
                round_up(align_of(&column, overrides)?, size_of(&column, overrides)?)?;
            u32::from(*columns).checked_mul(element_size)?
        }
        // A runtime-sized array has no fixed footprint, so neither does anything holding it.
        Ty::Array(element, Some(count)) => {
            let element_size =
                round_up(align_of(element, overrides)?, size_of(element, overrides)?)?;
            count.checked_mul(element_size)?
        }
        Ty::Struct(name, fields) => {
            let members = struct_layout(name, fields, overrides)?;
            let last = members.last()?;
            round_up(
                members.iter().map(|member| member.align).max().unwrap_or(1),
                last.offset.checked_add(last.size)?,
            )?
        }
        _ => return None,
    })
}

/// Offsets, alignments and sizes for each member in declaration order. Returns `None` if any
/// member is not host-shareable, since the offsets after it would be meaningless.
pub fn struct_layout(
    name: &str,
    fields: &[(String, Ty)],
    overrides: &MemberOverrides,
) -> Option<Vec<MemberLayout>> {
    let attributes = overrides.get(name);
    let mut members = Vec::with_capacity(fields.len());
    let mut offset = 0;
    for (index, (_, ty)) in fields.iter().enumerate() {
        let (align_attribute, size_attribute) = attributes
            .and_then(|list| list.get(index))
            .copied()
            .unwrap_or((None, None));
        let align = match align_attribute {
            Some(value) => value,
            None => align_of(ty, overrides)?,
        };
        let size = match size_attribute {
            Some(value) => value,
            None => size_of(ty, overrides)?,
        };
        offset = round_up(align, offset)?;
        members.push(MemberLayout {
            offset,
            align,
            size,
        });
        offset = offset.checked_add(size)?;
    }
    (!members.is_empty()).then_some(members)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{MemberOverrides, align_of, size_of, struct_layout};
    use crate::ty::Ty;

    fn vector(size: u8, element: Ty) -> Ty {
        Ty::Vector(size, Box::new(element))
    }

    fn matrix(columns: u8, rows: u8) -> Ty {
        Ty::Matrix(columns, rows, Box::new(Ty::F32))
    }

    fn none() -> MemberOverrides {
        HashMap::new()
    }

    #[test]
    fn scalar_and_vector_rules_match_the_specification() {
        let empty = none();
        for (ty, align, size) in [
            (Ty::F32, 4, 4),
            (Ty::I32, 4, 4),
            (Ty::U32, 4, 4),
            (Ty::F16, 2, 2),
            (vector(2, Ty::F32), 8, 8),
            (vector(3, Ty::F32), 16, 12),
            (vector(4, Ty::F32), 16, 16),
            (vector(2, Ty::F16), 4, 4),
            (vector(3, Ty::F16), 8, 6),
            (vector(4, Ty::F16), 8, 8),
        ] {
            assert_eq!(align_of(&ty, &empty), Some(align), "align of {ty:?}");
            assert_eq!(size_of(&ty, &empty), Some(size), "size of {ty:?}");
        }
    }

    #[test]
    fn matrix_rules_match_the_specification() {
        let empty = none();
        // Straight from the WGSL alignment and size table.
        for (columns, rows, align, size) in [
            (2, 2, 8, 16),
            (3, 2, 8, 24),
            (4, 2, 8, 32),
            (2, 3, 16, 32),
            (3, 3, 16, 48),
            (4, 3, 16, 64),
            (2, 4, 16, 32),
            (3, 4, 16, 48),
            (4, 4, 16, 64),
        ] {
            let ty = matrix(columns, rows);
            assert_eq!(
                align_of(&ty, &empty),
                Some(align),
                "align of mat{columns}x{rows}<f32>"
            );
            assert_eq!(
                size_of(&ty, &empty),
                Some(size),
                "size of mat{columns}x{rows}<f32>"
            );
        }
    }

    #[test]
    fn vec3_followed_by_scalar_packs_into_the_padding() {
        let empty = none();
        let fields = vec![
            ("position".to_owned(), vector(3, Ty::F32)),
            ("radius".to_owned(), Ty::F32),
        ];
        let members = struct_layout("Sphere", &fields, &empty).unwrap();
        assert_eq!(members[0].offset, 0);
        assert_eq!(members[0].size, 12);
        // The scalar slots into the four bytes of vec3 padding rather than starting at 16.
        assert_eq!(members[1].offset, 12);

        let ty = Ty::Struct("Sphere".to_owned(), fields);
        assert_eq!(align_of(&ty, &empty), Some(16));
        assert_eq!(size_of(&ty, &empty), Some(16));
    }

    #[test]
    fn scalar_followed_by_vec3_pads_to_the_next_boundary() {
        let empty = none();
        let fields = vec![
            ("radius".to_owned(), Ty::F32),
            ("position".to_owned(), vector(3, Ty::F32)),
        ];
        let members = struct_layout("Sphere", &fields, &empty).unwrap();
        assert_eq!(members[0].offset, 0);
        // 12 bytes of padding are inserted here, which is the bug this hint exposes.
        assert_eq!(members[1].offset, 16);
        assert_eq!(
            size_of(&Ty::Struct("Sphere".to_owned(), fields), &empty),
            Some(32)
        );
    }

    #[test]
    fn attributes_override_natural_alignment_and_size() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "Padded".to_owned(),
            vec![(None, Some(32)), (Some(64), None)],
        );
        let fields = vec![("head".to_owned(), Ty::F32), ("tail".to_owned(), Ty::F32)];
        let members = struct_layout("Padded", &fields, &overrides).unwrap();
        assert_eq!(members[0].size, 32);
        assert_eq!(members[1].align, 64);
        assert_eq!(members[1].offset, 64);
        assert_eq!(
            size_of(&Ty::Struct("Padded".to_owned(), fields), &overrides),
            Some(128)
        );
    }

    #[test]
    fn nested_structs_are_measured_with_their_own_attributes() {
        let mut overrides = HashMap::new();
        overrides.insert("Inner".to_owned(), vec![(None, Some(16))]);
        let inner = Ty::Struct("Inner".to_owned(), vec![("value".to_owned(), Ty::F32)]);
        assert_eq!(size_of(&inner, &overrides), Some(16));

        let outer = vec![("inner".to_owned(), inner), ("flag".to_owned(), Ty::U32)];
        let members = struct_layout("Outer", &outer, &overrides).unwrap();
        assert_eq!(members[1].offset, 16, "nested @size must push the sibling");
    }

    #[test]
    fn arrays_use_the_element_stride() {
        let empty = none();
        // vec3<f32> elements are strided to 16, not packed at 12.
        let array = Ty::Array(Box::new(vector(3, Ty::F32)), Some(4));
        assert_eq!(align_of(&array, &empty), Some(16));
        assert_eq!(size_of(&array, &empty), Some(64));

        let scalars = Ty::Array(Box::new(Ty::F32), Some(3));
        assert_eq!(size_of(&scalars, &empty), Some(12));
    }

    #[test]
    fn non_host_shareable_types_have_no_layout() {
        let empty = none();
        assert_eq!(size_of(&Ty::Bool, &empty), None);
        assert_eq!(size_of(&Ty::Sampler, &empty), None);
        assert_eq!(size_of(&Ty::Texture, &empty), None);
        // Runtime-sized arrays have no fixed footprint.
        assert_eq!(size_of(&Ty::Array(Box::new(Ty::F32), None), &empty), None);
        // And neither does a struct containing one.
        let fields = vec![("data".to_owned(), Ty::Array(Box::new(Ty::F32), None))];
        assert_eq!(struct_layout("Buffer", &fields, &empty), None);
    }

    #[test]
    fn huge_arrays_overflow_to_no_hint_instead_of_panicking() {
        let empty = none();
        // 400_000_000 * 16 bytes = 6.4e9, which overflows u32 (max ~4.29e9).
        let array = Ty::Array(Box::new(vector(4, Ty::F32)), Some(400_000_000));
        assert_eq!(size_of(&array, &empty), None);

        let fields = vec![("data".to_owned(), array)];
        assert_eq!(struct_layout("Huge", &fields, &empty), None);
        assert_eq!(
            size_of(&Ty::Struct("Huge".to_owned(), fields), &empty),
            None
        );

        // A normal struct is unaffected by the overflow checks.
        let normal_fields = vec![
            ("position".to_owned(), vector(3, Ty::F32)),
            ("radius".to_owned(), Ty::F32),
        ];
        let normal = Ty::Struct("Sphere".to_owned(), normal_fields);
        assert_eq!(align_of(&normal, &empty), Some(16));
        assert_eq!(size_of(&normal, &empty), Some(16));
    }

    #[test]
    fn accumulated_size_overrides_overflowing_the_offset_accumulator_yield_no_layout() {
        // Two members each carrying an explicit `@size(2_147_483_648)` (2^31): the natural
        // sizes are irrelevant, but summing the overrides into the offset accumulator still
        // overflows u32 (2^31 + 2^31 = 2^32), which is exactly the `offset.checked_add(size)?`
        // site the align/size rescue (finding 2) doesn't touch.
        let mut overrides = HashMap::new();
        overrides.insert(
            "Overflow".to_owned(),
            vec![(None, Some(2_147_483_648)), (None, Some(2_147_483_648))],
        );
        let fields = vec![("a".to_owned(), Ty::F32), ("b".to_owned(), Ty::F32)];
        assert_eq!(struct_layout("Overflow", &fields, &overrides), None);
    }

    #[test]
    fn huge_align_override_after_a_large_offset_overflows_round_up() {
        // The first member's `@size` pushes the offset just past a huge `@align` on the
        // second member, so rounding the offset up to that alignment (round_up's
        // `div_ceil(..).checked_mul(..)`) overflows u32 instead of landing on a valid offset.
        let mut overrides = HashMap::new();
        overrides.insert(
            "Overflow".to_owned(),
            vec![(None, Some(3_000_000_001)), (Some(3_000_000_000), None)],
        );
        let fields = vec![("a".to_owned(), Ty::F32), ("b".to_owned(), Ty::F32)];
        assert_eq!(struct_layout("Overflow", &fields, &overrides), None);
    }

    #[test]
    fn explicit_size_rescues_a_member_whose_natural_layout_overflows() {
        // The array's natural size (400_000_000 * 16 bytes) overflows u32 on its own — see
        // `huge_arrays_overflow_to_no_hint_instead_of_panicking` — but an explicit `@size`
        // makes that natural computation irrelevant, so the struct must still get a layout.
        let mut overrides = HashMap::new();
        overrides.insert("Rescued".to_owned(), vec![(None, Some(64))]);
        let array = Ty::Array(Box::new(vector(4, Ty::F32)), Some(400_000_000));
        let fields = vec![("data".to_owned(), array)];
        let members = struct_layout("Rescued", &fields, &overrides).unwrap();
        assert_eq!(members[0].offset, 0);
        assert_eq!(members[0].align, 16);
        assert_eq!(members[0].size, 64);
    }
}

/// Differential test against naga, which computes WGSL layout independently.
///
/// The unit tests above check this module against the specification's table, which proves the
/// table was transcribed correctly — not that the algorithm is right. naga is a real, separately
/// written implementation used by wgpu to lay out actual GPU buffers, so agreeing with it is the
/// evidence that matters for a feature whose entire value is correctness.
#[cfg(test)]
mod naga_oracle {
    use std::{collections::HashMap, fs, path::PathBuf};

    use walkdir::WalkDir;

    use super::{align_of, size_of, struct_layout};
    use crate::{
        index::struct_member_overrides, ty::Ty, ty::TypeEnvironment, ty::collect_struct_types,
    };

    #[test]
    fn corpus_struct_layouts_match_naga() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
        if !root.exists() {
            return;
        }
        let mut compared = 0;
        let mut mismatches = Vec::new();

        for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|extension| extension.to_str()) != Some("wgsl")
            {
                continue;
            }
            let Ok(source) = fs::read_to_string(path) else {
                continue;
            };
            let Ok(naga_module) = naga::front::wgsl::parse_str(&source) else {
                continue;
            };
            let Ok(parsed) = wgsl_parse::parse_str(&source) else {
                continue;
            };
            let mut layouter = naga::proc::Layouter::default();
            if layouter.update(naga_module.to_ctx()).is_err() {
                continue;
            }

            let overrides = struct_member_overrides(&parsed);
            let ours = collect_struct_types(&parsed, TypeEnvironment::default())
                .into_iter()
                .collect::<HashMap<_, _>>();

            for (handle, ty) in naga_module.types.iter() {
                let Some(name) = &ty.name else {
                    continue;
                };
                let naga::TypeInner::Struct { members, span } = &ty.inner else {
                    continue;
                };
                let Some(fields) = ours.get(name) else {
                    continue;
                };
                // We return None for anything not host-shareable; naga has nothing to compare.
                let Some(layout) = struct_layout(name, fields, &overrides) else {
                    continue;
                };
                if layout.len() != members.len() {
                    continue;
                }
                compared += 1;

                for (index, (mine, theirs)) in layout.iter().zip(members).enumerate() {
                    if mine.offset != theirs.offset {
                        mismatches.push(format!(
                            "{}: {name}.{} offset {} but naga says {}",
                            path.display(),
                            index,
                            mine.offset,
                            theirs.offset
                        ));
                    }
                }

                let whole = Ty::Struct(name.clone(), fields.clone());
                if let Some(size) = size_of(&whole, &overrides)
                    && size != *span
                {
                    mismatches.push(format!(
                        "{}: sizeof({name}) = {size} but naga says {span}",
                        path.display()
                    ));
                }
                // naga's IR keeps each member's computed offset but discards its `@align`
                // attribute, so `Layouter` derives struct alignment from member *types* alone.
                // The specification defines AlignOfMember as the attribute when present, so the
                // two quantities only coincide without overrides — compare them only there.
                // Offsets and size above are unaffected: naga's front end applies `@align`
                // before the IR, and those agreed on these structs too.
                let overridden = overrides
                    .get(name)
                    .is_some_and(|members| members.iter().any(|(align, _)| align.is_some()));
                if let Some(alignment) = align_of(&whole, &overrides)
                    && !overridden
                    && alignment != layouter[handle].alignment.round_up(1)
                {
                    mismatches.push(format!(
                        "{}: alignof({name}) = {alignment} but naga says {}",
                        path.display(),
                        layouter[handle].alignment.round_up(1)
                    ));
                }
            }
        }

        eprintln!("compared {compared} structs against naga");
        assert!(
            compared >= 50,
            "only compared {compared} structs; the corpus is probably missing"
        );
        assert!(
            mismatches.is_empty(),
            "{} layout disagreement(s) with naga:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }
}
