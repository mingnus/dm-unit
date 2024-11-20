use crate::block_manager::*;
use crate::emulator::memory::*;
use crate::fixture::*;
use crate::guest::alloc_guest;
use crate::stubs::block_manager::*;
use crate::stubs::*;
use crate::test_runner::*;
use crate::wrappers::block_manager::*;
use crate::wrappers::btree::*;
use crate::wrappers::btree_cursor::*;
use crate::wrappers::space_map::*;
use crate::wrappers::transaction_manager::*;

use anyhow::{anyhow, ensure, Result};
use rand::prelude::SliceRandom;
use rand::SeedableRng;
use std::sync::Arc;

//-------------------------------

fn test_iterate_empty_btree(fix: &mut Fixture) -> Result<()> {
    standard_globals(fix)?;

    // TODO: check the expected error code (-ENODATA)
    let mut t = BTreeCursorTest::new(fix)?;
    ensure!(t.get_cursor().is_err());

    Ok(())
}

fn iterate_populated_btree(fix: &mut Fixture, nr_entries: usize) -> Result<()> {
    standard_globals(fix)?;

    let mut t = BTreeCursorTest::new(fix)?;

    let mut values: Vec<u64> = (0..nr_entries as u64).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    values.shuffle(&mut rng);
    for (k, v) in values.iter().enumerate() {
        t.insert(k as u64, *v)?;
    }

    let mut c = t.get_cursor()?;
    let mut last_err = None;
    for (k, v) in values.iter().enumerate() {
        match c.get_value() {
            Ok((k2, v2)) => {
                if k as u64 != k2 || *v != v2 {
                    last_err = Some(anyhow!(
                        "unexpectd key-value pair ({} {}), expected ({} {})",
                        k2,
                        v2,
                        k,
                        v
                    ));
                    break;
                }
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }

        if k == nr_entries - 1 {
            break;
        }

        if let Err(e) = c.next() {
            last_err = Some(e);
            break;
        }
    }

    c.end()?;

    if let Some(e) = last_err {
        return Err(e);
    }

    Ok(())
}

fn test_iterate_populated_btree(fix: &mut Fixture) -> Result<()> {
    iterate_populated_btree(fix, 1024)
}

#[allow(dead_code)]
pub struct BTreeCursorTest<'a> {
    pub fix: &'a mut Fixture,
    bm: Addr,
    tm: Addr,
    sm: Addr,
    sb: Option<Addr>,
    info: BTreeInfo<u64>,
    info_ptr: Addr,
    root: u64,
}

impl<'a> BTreeCursorTest<'a> {
    pub fn new(fix: &'a mut Fixture) -> Result<Self> {
        let bm = dm_bm_create(fix, 1024)?;
        let (tm, sm) = dm_tm_create(fix, bm, 0)?;

        // FIXME: we should increment the superblock within the sm

        let info = BTreeInfo {
            tm,
            levels: 1,
            vtype: BTreeValueType::<u64>::default(),
        };
        let info_ptr = alloc_guest(&mut fix.vm.mem, &info, PERM_READ | PERM_WRITE)?;
        let root = dm_btree_empty(fix, &info)?;

        Ok(BTreeCursorTest {
            fix,
            bm,
            tm,
            sm,
            sb: None,
            info,
            info_ptr,
            root,
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

    pub fn insert(&mut self, key: u64, value: u64) -> Result<()> {
        let ks = vec![key];
        self.root = dm_btree_insert(self.fix, &self.info, self.root, &ks, &value)?;
        Ok(())
    }

    pub fn get_bm(&self) -> Arc<BlockManager> {
        get_bm(self.fix, self.bm)
    }

    pub fn get_cursor(&mut self) -> Result<BTreeCursor<u64>> {
        BTreeCursor::new(self.fix, self.info_ptr, self.root, true)
    }
}

impl Drop for BTreeCursorTest<'_> {
    fn drop(&mut self) {
        self.fix
            .vm
            .mem
            .free(self.info_ptr)
            .expect("free dm_btree_info");
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
        "/pdata/btree_cursor/",
        test!("iterate/empty", test_iterate_empty_btree)
        test!("iterate/populated", test_iterate_populated_btree)
    };

    Ok(())
}

//-------------------------------
