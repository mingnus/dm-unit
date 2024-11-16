use crate::emulator::memory::*;
use crate::emulator::riscv::*;
use crate::fixture::*;
use crate::guest::*;
use crate::wrappers::btree::*;

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io;
use std::io::{Read, Write};

use Reg::*;

//-------------------------------

pub struct ArrayInfo<G: Guest> {
    pub tm: Addr,
    pub vtype: BTreeValueType<G>,
    pub btree_info: BTreeInfo<u64>,
}

impl<G: Guest> Guest for ArrayInfo<G> {
    fn guest_len() -> usize {
        8 + BTreeValueType::<G>::guest_len() + BTreeInfo::<u64>::guest_len()
    }

    fn pack<W: Write>(&self, w: &mut W, loc: Addr) -> io::Result<()> {
        w.write_u64::<LittleEndian>(self.tm.0)?;
        self.vtype.pack(w, Addr(loc.0 + 8))?;
        self.btree_info.pack(w, Addr(loc.0 + 16))
    }

    fn unpack<R: Read>(r: &mut R) -> io::Result<Self> {
        let tm = Addr(r.read_u64::<LittleEndian>()?);
        let vtype = BTreeValueType::unpack(r)?;
        let btree_info = BTreeInfo::<u64>::unpack(r)?;

        Ok(ArrayInfo {
            tm,
            vtype,
            btree_info,
        })
    }
}

pub fn auto_array_info<'a, G: Guest>(
    fix: &'a mut Fixture,
    info: &ArrayInfo<G>,
) -> Result<(AutoGPtr<'a>, Addr)> {
    auto_guest(fix, info, PERM_READ | PERM_WRITE)
}

pub fn dm_array_info_init<G: Guest>(
    fix: &mut Fixture,
    tm: Addr,
    value_type: &BTreeValueType<G>, // the contained type for the array
) -> Result<ArrayInfo<G>> {
    let (mut fix, info_ptr) = auto_alloc(fix, ArrayInfo::<G>::guest_len())?;
    let (mut fix, vt_ptr) = auto_guest(&mut fix, value_type, PERM_READ | PERM_WRITE)?;

    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, tm.0);
    fix.vm.set_reg(A2, vt_ptr.0);

    fix.call("dm_array_info_init")?;

    let info = read_guest::<ArrayInfo<G>>(&fix.vm.mem, info_ptr)?;
    Ok(info)
}

pub fn dm_array_empty<G: Guest>(fix: &mut Fixture, info: &ArrayInfo<G>) -> Result<u64> {
    let (mut fix, info_ptr) = auto_array_info(fix, info)?;
    let (mut fix, root_ptr) = auto_alloc(&mut fix, 8)?;

    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root_ptr.0);

    fix.call_with_errno("dm_array_empty")?;

    Ok(fix.vm.mem.read_into::<u64>(root_ptr, PERM_READ)?)
}

pub fn dm_array_resize<G: Guest>(
    fix: &mut Fixture,
    info: &ArrayInfo<G>,
    root: u64,
    old_size: u32,
    new_size: u32,
    value: &G,
) -> Result<u64> {
    let (mut fix, info_ptr) = auto_array_info(fix, info)?;
    let (mut fix, value_ptr) = auto_guest(&mut fix, value, PERM_READ | PERM_WRITE)?;
    let (mut fix, new_root) = auto_alloc(&mut fix, 8)?;

    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root);
    fix.vm.set_reg(A2, old_size as u64);
    fix.vm.set_reg(A3, new_size as u64);
    fix.vm.set_reg(A4, value_ptr.0);
    fix.vm.set_reg(A5, new_root.0);

    fix.call_with_errno("dm_array_resize")?;

    let new_root = fix.vm.mem.read_into::<u64>(new_root, PERM_READ)?;
    Ok(new_root)
}

pub fn dm_array_new() -> Result<u64> {
    unimplemented!();
}

pub fn dm_array_del<G: Guest>(fix: &mut Fixture, info: &ArrayInfo<G>, root: u64) -> Result<()> {
    let (mut fix, info_ptr) = auto_array_info(fix, info)?;
    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root);
    fix.call_with_errno("dm_array_del")
}

pub fn dm_array_set_value<G: Guest>(
    fix: &mut Fixture,
    info: &ArrayInfo<G>,
    root: u64,
    index: u32,
    value: &G,
) -> Result<u64> {
    let (mut fix, info_ptr) = auto_array_info(fix, info)?;
    let (mut fix, value_ptr) = auto_guest(&mut fix, value, PERM_READ | PERM_WRITE)?;
    let (mut fix, new_root) = auto_alloc(&mut fix, 8)?;

    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root);
    fix.vm.set_reg(A2, index as u64);
    fix.vm.set_reg(A3, value_ptr.0);
    fix.vm.set_reg(A4, new_root.0);

    fix.call_with_errno("dm_array_set_value")?;

    let new_root = fix.vm.mem.read_into::<u64>(new_root, PERM_READ)?;
    Ok(new_root)
}

pub fn dm_array_get_value<G: Guest>(
    fix: &mut Fixture,
    info: &ArrayInfo<G>,
    root: u64,
    index: u32,
) -> Result<G> {
    let (mut fix, info_ptr) = auto_array_info(fix, info)?;
    let (mut fix, value_ptr) = auto_alloc(&mut fix, G::guest_len())?;

    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root);
    fix.vm.set_reg(A2, index as u64);
    fix.vm.set_reg(A3, value_ptr.0);

    fix.call_with_errno("dm_array_get_value")?;

    let value = read_guest::<G>(&fix.vm.mem, value_ptr)?;
    Ok(value)
}

//-------------------------------
