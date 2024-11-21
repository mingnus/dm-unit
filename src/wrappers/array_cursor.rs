use crate::emulator::memory::*;
use crate::emulator::riscv::*;
use crate::fixture::*;
use crate::guest::*;

use anyhow::Result;
use std::marker::PhantomData;

use Reg::*;

//-------------------------------

// sizeof(struct dm_array_cursor)
const ARRAY_CURSOR_GUEST_LEN: usize = 320;

pub struct ArrayCursor<'a, G: Guest> {
    fix: &'a mut Fixture,
    cursor_ptr: Addr,
    rust_value_type: PhantomData<G>,
}

impl<'a, G: Guest> ArrayCursor<'a, G> {
    pub fn new(fix: &'a mut Fixture, info_ptr: Addr, root: u64) -> Result<Self> {
        let cursor_ptr = fix
            .vm
            .mem
            .alloc_bytes(vec![0; ARRAY_CURSOR_GUEST_LEN], PERM_READ | PERM_WRITE)?;
        if let Err(e) = dm_array_cursor_begin(fix, info_ptr, root, cursor_ptr) {
            let _ = dm_array_cursor_end(fix, cursor_ptr);
            let _ = fix.vm.mem.free(cursor_ptr);
            return Err(e);
        }
        Ok(Self {
            fix,
            cursor_ptr,
            rust_value_type: PhantomData,
        })
    }

    // don't implement the std Iterator so that we could test at a finer granularity.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<()> {
        dm_array_cursor_next(self.fix, self.cursor_ptr)
    }

    pub fn skip(&mut self, count: u32) -> Result<()> {
        dm_array_cursor_skip(self.fix, self.cursor_ptr, count)
    }

    pub fn get_value(&mut self) -> Result<G> {
        dm_array_cursor_get_value::<G>(self.fix, self.cursor_ptr)
    }

    pub fn end(&mut self) -> Result<()> {
        dm_array_cursor_end(self.fix, self.cursor_ptr)
    }
}

impl<G: Guest> Drop for ArrayCursor<'_, G> {
    fn drop(&mut self) {
        self.fix
            .vm
            .mem
            .free(self.cursor_ptr)
            .expect("ArrayCursor drop");
    }
}

//-------------------------------

pub fn dm_array_cursor_begin(
    fix: &mut Fixture,
    info_ptr: Addr,
    root: u64,
    cursor_ptr: Addr,
) -> Result<()> {
    fix.vm.set_reg(A0, info_ptr.0);
    fix.vm.set_reg(A1, root);
    fix.vm.set_reg(A2, cursor_ptr.0);
    fix.call_with_errno("dm_array_cursor_begin")
}

pub fn dm_array_cursor_end(fix: &mut Fixture, cursor_ptr: Addr) -> Result<()> {
    fix.vm.set_reg(A0, cursor_ptr.0);
    fix.call("dm_array_cursor_end")
}

pub fn dm_array_cursor_next(fix: &mut Fixture, cursor_ptr: Addr) -> Result<()> {
    fix.vm.set_reg(A0, cursor_ptr.0);
    fix.call_with_errno("dm_array_cursor_next")
}

pub fn dm_array_cursor_skip(fix: &mut Fixture, cursor_ptr: Addr, count: u32) -> Result<()> {
    fix.vm.set_reg(A0, cursor_ptr.0);
    fix.vm.set_reg(A1, count as u64);
    fix.call_with_errno("dm_array_cursor_skip")
}

pub fn dm_array_cursor_get_value<G: Guest>(fix: &mut Fixture, cursor_ptr: Addr) -> Result<G> {
    let (mut fix, ppvalue) = auto_alloc(fix, 8)?;

    fix.vm.set_reg(A0, cursor_ptr.0);
    fix.vm.set_reg(A1, ppvalue.0);

    fix.call("dm_array_cursor_get_value")?;

    let value_ptr = Addr(read_guest::<u64>(&fix.vm.mem, ppvalue)?);
    let value = read_guest::<G>(&fix.vm.mem, value_ptr)?;
    Ok(value)
}

//-------------------------------
