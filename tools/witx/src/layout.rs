use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SizeAlign {
    pub size: usize,
    pub align: usize,
}

impl SizeAlign {
    fn zero() -> SizeAlign {
        SizeAlign { size: 0, align: 0 }
    }
    fn append_field(&mut self, other: &SizeAlign) {
        self.align = self.align.max(other.align);
        self.size = align_to(self.size, other.align);
        self.size += other.size;
    }
}

pub trait Layout {
    fn mem_size_align(&self) -> SizeAlign;
    fn mem_size(&self) -> usize {
        self.mem_size_align().size
    }
    fn mem_align(&self) -> usize {
        self.mem_size_align().align
    }
}

impl TypeRef {
    fn layout(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> SizeAlign {
        if let Some(hit) = cache.get(self) {
            return *hit;
        }
        let layout = match &self {
            TypeRef::Name(nt) => nt.layout(cache),
            TypeRef::Value(v) => v.layout(cache),
        };
        cache.insert(self.clone(), layout);
        layout
    }
}

impl Layout for TypeRef {
    fn mem_size_align(&self) -> SizeAlign {
        let mut cache = HashMap::new();
        self.layout(&mut cache)
    }
}

impl NamedType {
    fn layout(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> SizeAlign {
        self.tref.layout(cache)
    }
}
impl Layout for NamedType {
    fn mem_size_align(&self) -> SizeAlign {
        let mut cache = HashMap::new();
        self.layout(&mut cache)
    }
}

impl Type {
    fn layout(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> SizeAlign {
        match &self {
            Type::Enum(e) => e.repr.mem_size_align(),
            Type::Flags(f) => f.repr.mem_size_align(),
            Type::Record(s) => s.layout(cache),
            Type::Variant(s) => s.mem_size_align(),
            Type::Union(u) => u.layout(cache),
            Type::Handle(h) => h.mem_size_align(),
            Type::List { .. } => BuiltinType::String.mem_size_align(),
            Type::Pointer { .. } | Type::ConstPointer { .. } => BuiltinType::U32.mem_size_align(),
            Type::Builtin(b) => b.mem_size_align(),
        }
    }
}

impl Layout for Type {
    fn mem_size_align(&self) -> SizeAlign {
        let mut cache = HashMap::new();
        self.layout(&mut cache)
    }
}

impl Layout for IntRepr {
    fn mem_size_align(&self) -> SizeAlign {
        match self {
            IntRepr::U8 => BuiltinType::U8.mem_size_align(),
            IntRepr::U16 => BuiltinType::U16.mem_size_align(),
            IntRepr::U32 => BuiltinType::U32.mem_size_align(),
            IntRepr::U64 => BuiltinType::U64.mem_size_align(),
        }
    }
}

pub struct RecordMemberLayout<'a> {
    pub member: &'a RecordMember,
    pub offset: usize,
}

impl RecordDatatype {
    pub fn member_layout(&self) -> Vec<RecordMemberLayout> {
        self.member_layout_(&mut HashMap::new()).1
    }

    fn member_layout_(
        &self,
        cache: &mut HashMap<TypeRef, SizeAlign>,
    ) -> (SizeAlign, Vec<RecordMemberLayout>) {
        let mut members = Vec::new();
        let mut sa = SizeAlign::zero();
        for m in self.members.iter() {
            let member = m.tref.layout(cache);
            sa.append_field(&member);
            members.push(RecordMemberLayout {
                member: m,
                offset: sa.size - member.size,
            });
        }
        sa.size = align_to(sa.size, sa.align);
        (sa, members)
    }

    fn layout(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> SizeAlign {
        self.member_layout_(cache).0
    }
}

impl Layout for RecordDatatype {
    fn mem_size_align(&self) -> SizeAlign {
        let mut cache = HashMap::new();
        self.layout(&mut cache)
    }
}

impl Layout for Variant {
    fn mem_size_align(&self) -> SizeAlign {
        let mut max = SizeAlign { size: 0, align: 0 };
        for case in self.cases.iter() {
            let mut size = BuiltinType::S32.mem_size_align();
            if let Some(payload) = &case.tref {
                size.append_field(&payload.mem_size_align());
            }
            max.size = max.size.max(size.size);
            max.align = max.align.max(size.align);
        }
        max
    }
}

/// If the next free byte in the struct is `offs`, and the next
/// element has alignment `alignment`, determine the offset at
/// which to place that element.
fn align_to(offs: usize, alignment: usize) -> usize {
    offs + alignment - 1 - ((offs + alignment - 1) % alignment)
}

#[cfg(test)]
mod test {
    use super::align_to;
    #[test]
    fn align() {
        assert_eq!(0, align_to(0, 1));
        assert_eq!(0, align_to(0, 2));
        assert_eq!(0, align_to(0, 4));
        assert_eq!(0, align_to(0, 8));

        assert_eq!(1, align_to(1, 1));
        assert_eq!(2, align_to(1, 2));
        assert_eq!(4, align_to(1, 4));
        assert_eq!(8, align_to(1, 8));

        assert_eq!(2, align_to(2, 1));
        assert_eq!(2, align_to(2, 2));
        assert_eq!(4, align_to(2, 4));
        assert_eq!(8, align_to(2, 8));

        assert_eq!(5, align_to(5, 1));
        assert_eq!(6, align_to(5, 2));
        assert_eq!(8, align_to(5, 4));
        assert_eq!(8, align_to(5, 8));
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct UnionLayout {
    pub tag_size: usize,
    pub tag_align: usize,
    pub contents_offset: usize,
    pub contents_size: usize,
    pub contents_align: usize,
}

impl Layout for UnionLayout {
    fn mem_size_align(&self) -> SizeAlign {
        let align = std::cmp::max(self.tag_align, self.contents_align);
        let size = align_to(self.contents_offset + self.contents_size, align);
        SizeAlign { size, align }
    }
}

impl UnionDatatype {
    pub fn union_layout(&self) -> UnionLayout {
        let mut cache = HashMap::new();
        self.union_layout_(&mut cache)
    }
    fn union_layout_(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> UnionLayout {
        let tag = self.tag.layout(cache);

        let variant_sas = self
            .variants
            .iter()
            .filter_map(|v| v.tref.as_ref().map(|t| t.layout(cache)))
            .collect::<Vec<SizeAlign>>();

        let contents_size = variant_sas.iter().map(|sa| sa.size).max().unwrap_or(0);
        let contents_align = variant_sas.iter().map(|sa| sa.align).max().unwrap_or(1);

        UnionLayout {
            tag_size: tag.size,
            tag_align: tag.align,
            contents_offset: align_to(tag.size, contents_align),
            contents_size,
            contents_align,
        }
    }

    fn layout(&self, cache: &mut HashMap<TypeRef, SizeAlign>) -> SizeAlign {
        self.union_layout_(cache).mem_size_align()
    }
}

impl Layout for UnionDatatype {
    fn mem_size_align(&self) -> SizeAlign {
        let mut cache = HashMap::new();
        self.layout(&mut cache)
    }
}

impl Layout for HandleDatatype {
    fn mem_size_align(&self) -> SizeAlign {
        BuiltinType::U32.mem_size_align()
    }
}

impl Layout for BuiltinType {
    fn mem_size_align(&self) -> SizeAlign {
        match self {
            BuiltinType::String => SizeAlign { size: 8, align: 4 }, // Pointer and Length
            BuiltinType::U8 | BuiltinType::S8 | BuiltinType::Char8 => {
                SizeAlign { size: 1, align: 1 }
            }
            BuiltinType::U16 | BuiltinType::S16 => SizeAlign { size: 2, align: 2 },
            BuiltinType::USize | BuiltinType::U32 | BuiltinType::S32 | BuiltinType::F32 => {
                SizeAlign { size: 4, align: 4 }
            }
            BuiltinType::U64 | BuiltinType::S64 | BuiltinType::F64 => {
                SizeAlign { size: 8, align: 8 }
            }
        }
    }
}
