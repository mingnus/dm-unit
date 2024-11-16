use crate::block_manager::*;
use crate::emulator::memory::*;
use crate::fixture::*;
use crate::stubs::block_manager::*;
use crate::stubs::*;
use crate::test_runner::*;
use crate::wrappers::array::*;
use crate::wrappers::block_manager::*;
use crate::wrappers::btree::BTreeValueType;
use crate::wrappers::space_map::*;
use crate::wrappers::transaction_manager::*;

use anyhow::{anyhow, ensure, Result};
use rand::prelude::SliceRandom;
use rand::SeedableRng;
use std::sync::Arc;

//-------------------------------

const MAX_U64_ENTRIES_PER_BLOCK: u32 = 509;

// Delete an empty array.
fn test_del_empty(fix: &mut Fixture) -> Result<()> {
    standard_globals(fix)?;

    let mut at = ArrayTest::new(fix)?;
    at.begin()?;
    at.delete()
}

//-------------------------------

// TODO: check array entries
// TODO: use random numbers for the value
fn test_resize_array(fix: &mut Fixture, old_size: u32, new_size: u32) -> Result<()> {
    standard_globals(fix)?;

    let mut at = ArrayTest::new(fix)?;

    if old_size > 0 {
        at.begin()?;
        at.resize(old_size, 0)?;
        at.commit()?;
    }

    at.begin()?;
    at.resize(new_size, 0)?;
    at.commit()?;

    Ok(())
}

fn test_expand_from_empty_within_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 0, 101)
}

fn test_expand_from_empty_to_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 0, MAX_U64_ENTRIES_PER_BLOCK)
}

fn test_expand_from_empty_across_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 0, MAX_U64_ENTRIES_PER_BLOCK + 101)
}

fn test_expand_from_empty_to_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 0, MAX_U64_ENTRIES_PER_BLOCK * 2)
}

fn test_expand_from_empty_across_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 0, MAX_U64_ENTRIES_PER_BLOCK * 2 + 101)
}

fn test_expand_from_non_empty_within_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 97, 101)
}

fn test_expand_from_non_empty_to_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 97, MAX_U64_ENTRIES_PER_BLOCK)
}

fn test_expand_from_non_empty_across_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 97, MAX_U64_ENTRIES_PER_BLOCK + 101)
}

fn test_expand_from_non_empty_to_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 97, MAX_U64_ENTRIES_PER_BLOCK * 2)
}

fn test_expand_from_non_empty_across_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(fix, 97, MAX_U64_ENTRIES_PER_BLOCK * 2 + 101)
}

fn test_expand_from_aligned_within_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(
        fix,
        MAX_U64_ENTRIES_PER_BLOCK,
        MAX_U64_ENTRIES_PER_BLOCK + 101,
    )
}

fn test_expand_from_aligned_to_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(
        fix,
        MAX_U64_ENTRIES_PER_BLOCK,
        MAX_U64_ENTRIES_PER_BLOCK * 2,
    )
}

fn test_expand_from_aligned_across_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(
        fix,
        MAX_U64_ENTRIES_PER_BLOCK,
        MAX_U64_ENTRIES_PER_BLOCK * 2 + 101,
    )
}

fn test_expand_from_aligned_to_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(
        fix,
        MAX_U64_ENTRIES_PER_BLOCK,
        MAX_U64_ENTRIES_PER_BLOCK * 3,
    )
}

fn test_expand_from_aligned_across_next_boundary(fix: &mut Fixture) -> Result<()> {
    test_resize_array(
        fix,
        MAX_U64_ENTRIES_PER_BLOCK,
        MAX_U64_ENTRIES_PER_BLOCK * 3 + 101,
    )
}

//-------------------------------

fn test_set_values(fix: &mut Fixture, values: &[(u32, u64)]) -> Result<()> {
    standard_globals(fix)?;

    let mut at = ArrayTest::new(fix)?;
    at.begin()?;

    let size = values.iter().map(|(i, _)| *i).max().map_or(0, |s| s + 1);
    at.resize(size, 0)?;

    values.iter().try_for_each(|(i, v)| at.set_value(*i, v))?;

    // TODO: check the rest of the entries are not affected
    values.iter().try_for_each(|(i, v)| {
        ensure!(at.get_value(*i)? == *v);
        Ok(())
    })?;

    Ok(())
}

fn test_set_values_forward(fix: &mut Fixture) -> Result<()> {
    let mut values: Vec<u64> = (0..1024u64).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    values.shuffle(&mut rng);
    let values: Vec<(u32, u64)> = values
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i as u32, v))
        .collect();

    test_set_values(fix, &values)
}

fn test_set_values_backward(fix: &mut Fixture) -> Result<()> {
    let mut values: Vec<u64> = (0..1024u64).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    values.shuffle(&mut rng);
    let values: Vec<(u32, u64)> = values
        .into_iter()
        .enumerate()
        .rev()
        .map(|(i, v)| (i as u32, v))
        .collect();

    test_set_values(fix, &values)
}

fn test_set_values_random(fix: &mut Fixture) -> Result<()> {
    let mut values: Vec<u64> = (0..1024u64).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    values.shuffle(&mut rng);
    let mut values: Vec<(u32, u64)> = values
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i as u32, v))
        .collect();
    values.shuffle(&mut rng);

    test_set_values(fix, &values)
}

//-------------------------------

#[allow(dead_code)]
pub struct ArrayTest<'a> {
    pub fix: &'a mut Fixture,
    bm: Addr,
    tm: Addr,
    sm: Addr,
    sb: Option<Addr>,
    info: ArrayInfo<u64>,
    root: u64,
    array_size: u32,
}

impl<'a> ArrayTest<'a> {
    pub fn new(fix: &'a mut Fixture) -> Result<Self> {
        let bm = dm_bm_create(fix, 1024)?;
        let (tm, sm) = dm_tm_create(fix, bm, 0)?;

        // FIXME: we should increment the superblock within the sm

        let vtype = BTreeValueType::<u64>::default(); // contained type for the array
        let info = dm_array_info_init(fix, tm, &vtype)?;
        let root = dm_array_empty(fix, &info)?;

        Ok(ArrayTest {
            fix,
            bm,
            tm,
            sm,
            sb: None,
            info,
            root,
            array_size: 0,
        })
    }

    pub fn begin(&mut self) -> Result<()> {
        if self.sb.is_some() {
            return Err(anyhow!("transaction already begun"));
        }

        // lock the superblock to prevent overwrite
        // FIXME: this should be moved to commit(), before dm_tm_commit(),
        // once we had increased the superblock ref count in the test constructor.
        self.sb = Some(dm_bm_write_lock_zero(self.fix, self.bm, 0, Addr(0))?);
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        dm_tm_pre_commit(self.fix, self.tm)?;
        dm_tm_commit(self.fix, self.tm, self.sb.unwrap())?;
        self.sb = None; // dm_tm_commit unlocked the superblock implicitly
        Ok(())
    }

    // This function takes ownership as the array is no longer valid
    pub fn delete(mut self) -> Result<()> {
        dm_array_del(self.fix, &self.info, self.root)?;
        self.commit()
    }

    pub fn set_value(&mut self, index: u32, value: &u64) -> Result<()> {
        self.root = dm_array_set_value(self.fix, &self.info, self.root, index, value)?;
        Ok(())
    }

    pub fn get_value(&mut self, index: u32) -> Result<u64> {
        dm_array_get_value(self.fix, &self.info, self.root, index)
    }

    pub fn resize(&mut self, new_size: u32, value: u64) -> Result<()> {
        self.root = dm_array_resize(
            self.fix,
            &self.info,
            self.root,
            self.array_size,
            new_size,
            &value,
        )?;
        self.array_size = new_size;
        Ok(())
    }

    pub fn get_bm(&self) -> Arc<BlockManager> {
        get_bm(self.fix, self.bm)
    }
}

impl Drop for ArrayTest<'_> {
    fn drop(&mut self) {
        if let Some(sb) = self.sb {
            dm_bm_unlock(self.fix, sb).expect("unlock superblock");
        }
        dm_tm_destroy(self.fix, self.tm).expect("destroy tm");
        sm_destroy(self.fix, self.sm).expect("destroy sm");
        dm_bm_destroy(self.fix, self.bm).expect("destroy bm");
    }
}

//-------------------------------

pub fn register_tests(tests: &mut TestSet) -> Result<()> {
    let kmodules = vec![PDATA_MOD];
    let mut prefix: Vec<&'static str> = Vec::new();

    macro_rules! test_section {
        ($path:expr, $($s:stmt)*) => {{
            prefix.push($path);
            $($s)*
            prefix.pop().unwrap();
        }}
    }

    macro_rules! test {
        ($path:expr, $func:expr) => {{
            prefix.push($path);
            let p = prefix.concat();
            prefix.pop().unwrap();
            tests.register(&p, Test::new(kmodules.clone(), Box::new($func)));
        }};
    }

    test_section! {
        "/pdata/array/",
        test!("del/empty", test_del_empty)

        test_section! {
            "resize/grow/from_empty/",
            test!("within_block", test_expand_from_empty_within_boundary)
            test!("to_boundary", test_expand_from_empty_to_boundary)
            test!("across_boundary", test_expand_from_empty_across_boundary)
            test!("to_next_boundary", test_expand_from_empty_to_next_boundary)
            test!("across_next_boundary", test_expand_from_empty_across_next_boundary)
        }

        test_section! {
            "resize/grow/from_non_empty/",
            test!("within_block", test_expand_from_non_empty_within_boundary)
            test!("to_boundary", test_expand_from_non_empty_to_boundary)
            test!("across_boundary", test_expand_from_non_empty_across_boundary)
            test!("to_next_boundary", test_expand_from_non_empty_to_next_boundary)
            test!("across_next_boundary", test_expand_from_non_empty_across_next_boundary)
        }

        test_section! {
            "resize/grow/from_aligned/",
            test!("within_block", test_expand_from_aligned_within_boundary)
            test!("to_boundary", test_expand_from_aligned_to_boundary)
            test!("across_boundary", test_expand_from_aligned_across_boundary)
            test!("to_next_boundary", test_expand_from_aligned_to_next_boundary)
            test!("across_next_boundary", test_expand_from_aligned_across_next_boundary)
        }

        test_section! {
            "set_values/",
            test!("forward", test_set_values_forward)
            test!("backward", test_set_values_backward)
            test!("random", test_set_values_random)
        }
    };

    Ok(())
}

//-------------------------------
