use crate::decode::*;
use crate::fixture::*;
use crate::guest::*;
use crate::memory::*;
use crate::stats::*;
use crate::stubs::block_manager::*;
use crate::stubs::*;
use crate::test_runner::*;
use crate::wrappers::block_manager::*;
use crate::wrappers::btree::*;
use crate::wrappers::transaction_manager::*;

use anyhow::{anyhow, ensure, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::*;
use nom::{number::complete::*, IResult};
use rand::prelude::*;
use rand::SeedableRng;
use std::collections::BTreeSet;
use std::io;
use std::io::{Cursor, Read, Write};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use thinp::io_engine::BLOCK_SIZE;
use thinp::pdata::btree;
use thinp::pdata::btree::*;
use thinp::pdata::btree_builder::*;
use thinp::pdata::btree_walker::*;
use thinp::pdata::unpack::*;

//-------------------------------

struct NoopVisitor {}

impl<V: Unpack> NodeVisitor<V> for NoopVisitor {
    fn visit(
        &self,
        _path: &[u64],
        _kr: &KeyRange,
        _header: &NodeHeader,
        _keys: &[u64],
        _values: &[V],
    ) -> btree::Result<()> {
        Ok(())
    }

    fn visit_again(&self, _path: &[u64], _b: u64) -> btree::Result<()> {
        Ok(())
    }

    fn end_walk(&self) -> btree::Result<()> {
        Ok(())
    }
}

#[allow(dead_code)]
fn check_btree(root: u64) -> Result<()> {
    let engine = get_bm()?.engine.clone();
    let walker = BTreeWalker::new(engine, false);
    let visitor = NoopVisitor {};
    let mut path = Vec::new();

    walker.walk::<NoopVisitor, Value64>(&mut path, &visitor, root)?;

    Ok(())
}

//-------------------------------

struct ResidencyVisitor {
    nr_entries: AtomicU32,
    nr_leaves: AtomicU32,
}

impl<V: Unpack> NodeVisitor<V> for ResidencyVisitor {
    fn visit(
        &self,
        _path: &[u64],
        _kr: &KeyRange,
        _header: &NodeHeader,
        keys: &[u64],
        _values: &[V],
    ) -> btree::Result<()> {
        self.nr_entries
            .fetch_add(keys.len() as u32, Ordering::SeqCst);
        self.nr_leaves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn visit_again(&self, _path: &[u64], _b: u64) -> btree::Result<()> {
        Ok(())
    }

    fn end_walk(&self) -> btree::Result<()> {
        Ok(())
    }
}

fn calc_max_entries<V: Unpack>() -> usize {
    let elt_size = 8 + V::disk_size() as usize; // key + value size
    ((BLOCK_SIZE - NodeHeader::disk_size() as usize) / elt_size) as usize
}

// Because this is a walk it implicitly checks the btree.  Returns
// average residency as a _percentage_.
fn calc_residency(root: u64) -> Result<usize> {
    let engine = get_bm()?.engine.clone();
    let walker = BTreeWalker::new(engine, false);
    let visitor = ResidencyVisitor {
        nr_entries: AtomicU32::new(0),
        nr_leaves: AtomicU32::new(0),
    };
    let mut path = Vec::new();

    walker.walk::<ResidencyVisitor, Value64>(&mut path, &visitor, root)?;

    let nr_entries = visitor.nr_entries.load(Ordering::SeqCst) as usize;
    let nr_leaves = visitor.nr_leaves.load(Ordering::SeqCst) as usize;
    let max_entries = calc_max_entries::<Value64>();

    let percent = (nr_entries * 100) / (max_entries * nr_leaves);

    Ok(percent)
}

//-------------------------------

// Used to confirm all expected keys are present in the tree.
struct EntryVisitor {
    seen: Mutex<BTreeSet<u64>>,
}

fn key_to_value(k: u64) -> u64 {
    k + 12345
}

impl NodeVisitor<Value64> for EntryVisitor {
    fn visit(
        &self,
        _path: &[u64],
        _kr: &KeyRange,
        _header: &NodeHeader,
        keys: &[u64],
        values: &[Value64],
    ) -> btree::Result<()> {
        for (i, k) in keys.iter().enumerate() {
            let v = values[i];
            if v.0 != key_to_value(*k) {
                return Err(BTreeError::ValueError(format!(
                    "Key has bad value: {} -> {}",
                    k, v.0
                )));
            }

            let mut seen = self.seen.lock().unwrap();
            seen.insert(*k);
        }

        Ok(())
    }

    fn visit_again(&self, _path: &[u64], _b: u64) -> btree::Result<()> {
        Ok(())
    }

    fn end_walk(&self) -> btree::Result<()> {
        Ok(())
    }
}

fn check_keys_present(root: u64, keys: &[u64]) -> Result<()> {
    let engine = get_bm()?.engine.clone();
    let walker = BTreeWalker::new(engine, false);
    let visitor = EntryVisitor {
        seen: Mutex::new(BTreeSet::new()),
    };

    let mut path = Vec::new();
    walker.walk::<EntryVisitor, Value64>(&mut path, &visitor, root)?;

    let seen = visitor.seen.lock().unwrap();
    for k in keys {
        if !seen.contains(k) {
            return Err(anyhow!("Key missing from btree: {}", *k));
        }
    }

    Ok(())
}

//-------------------------------

/// A little wrapper to let us store u64's in btrees.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Value64(u64);

impl Guest for Value64 {
    fn guest_len() -> usize {
        8
    }

    fn pack<W: Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_u64::<LittleEndian>(self.0)
    }

    fn unpack<R: Read>(r: &mut R) -> io::Result<Self> {
        let v = r.read_u64::<LittleEndian>()?;
        Ok(Value64(v))
    }
}

impl Unpack for Value64 {
    fn disk_size() -> u32 {
        8
    }

    fn unpack(data: &[u8]) -> IResult<&[u8], Self> {
        let (i, v) = le_u64(data)?;
        Ok((i, Value64(v)))
    }
}

impl Pack for Value64 {
    fn pack<W: WriteBytesExt>(&self, w: &mut W) -> Result<()> {
        w.write_u64::<LittleEndian>(self.0)?;
        Ok(())
    }
}

#[allow(dead_code)]
fn enable_traces(fix: &mut Fixture) -> Result<()> {
    let traces = [
        "btree_insert_raw",
        "dm_btree_cursor_begin",
        "dm_btree_cursor_end",
        "dm_btree_cursor_get_value",
        "dm_btree_cursor_next",
        "dm_btree_cursor_skip",
        "dm_btree_del",
        "dm_btree_empty",
        "dm_btree_find_highest_key",
        "dm_btree_find_lowest_key",
        "dm_btree_insert",
        "dm_btree_insert_notify",
        "dm_btree_lookup",
        "dm_btree_lookup_next",
        "dm_btree_lookup_next_single",
        "dm_btree_remove",
        "dm_btree_remove_leaves",
        "dm_btree_walk",
        "dm_sm_metadata_create",
        "dm_tm_create",
        "dm_tm_create_with_sm",
        "dm_tm_new_block",
        "dm_tm_unlock",
        "insert",
        "insert_at",
        "lower_bound",
        "metadata_ll_init_index",
        "shadow_current",
        "shadow_step",
        "sm_bootstrap_new_block",
        "sm_bootstrap_new_block",
        "sm_ll_extend",
        "sm_ll_init",
        "sm_ll_new_metadata",
    ];
    for t in &traces {
        fix.trace_func(t)?;
    }
    Ok(())
}

//-------------------------------

// Delete an empty tree.
fn test_del_empty(fix: &mut Fixture) -> Result<()> {
    standard_globals(fix)?;

    let bm = dm_bm_create(fix, 1024)?;
    let (tm, _sm) = dm_tm_create(fix, bm, 0)?;

    let vtype: BTreeValueType<Value64> = BTreeValueType {
        context: Addr(0),
        inc_fn: Addr(0),
        dec_fn: Addr(0),
        eq_fn: Addr(0),
        rust_value_type: PhantomData,
    };
    let info = BTreeInfo {
        tm,
        levels: 1,
        vtype,
    };

    let root = dm_btree_empty(fix, &info)?;
    dm_btree_del(fix, &info, root)?;
    Ok(())
}

#[allow(dead_code)]
struct BTreeTest<'a> {
    fix: &'a mut Fixture,
    bm: Addr,
    tm: Addr,
    sm: Addr,
    sb: Addr,
    info: BTreeInfo<Value64>,
    root: u64,
    baseline: Stats,
}

impl<'a> BTreeTest<'a> {
    fn new(fix: &'a mut Fixture) -> Result<Self> {
        let bm = dm_bm_create(fix, 1024)?;
        let (tm, sm) = dm_tm_create(fix, bm, 0)?;
        let sb = dm_bm_write_lock_zero(fix, bm, 0, Addr(0))?;

        // FIXME: we should increment the superblock within the sm

        let vtype: BTreeValueType<Value64> = BTreeValueType {
            context: Addr(0),
            inc_fn: Addr(0),
            dec_fn: Addr(0),
            eq_fn: Addr(0),
            rust_value_type: PhantomData,
        };
        let info = BTreeInfo {
            tm,
            levels: 1,
            vtype,
        };
        let root = dm_btree_empty(fix, &info)?;
        let baseline = Stats::collect_stats(fix);

        Ok(BTreeTest {
            fix,
            bm,
            tm,
            sm,
            sb,
            info,
            root,
            baseline,
        })
    }

    fn insert(&mut self, key: u64) -> Result<()> {
        let ks = vec![key];
        let v = Value64(key_to_value(key));
        self.root = dm_btree_insert(self.fix, &self.info, self.root, &ks, &v)?;
        Ok(())
    }

    fn lookup(&mut self, key: u64) -> Result<()> {
        let keys = vec![key];
        let v = dm_btree_lookup(self.fix, &self.info, self.root, &keys)?;
        ensure!(v == Value64(key_to_value(key)));
        Ok(())
    }

    // This uses Rust code, rather than doing look ups via the kernel
    // code.
    fn check_keys_present(&self, keys: &[u64]) -> Result<()> {
        check_keys_present(self.root, keys)
    }

    fn commit(&mut self) -> Result<()> {
        dm_tm_pre_commit(self.fix, self.tm)?;
        dm_tm_commit(self.fix, self.tm, self.sb)?;
        self.sb = dm_bm_write_lock_zero(self.fix, self.bm, 0, Addr(0))?;
        Ok(())
    }

    fn stats_start(&mut self) {
        self.baseline = Stats::collect_stats(self.fix);
    }

    fn stats_report(&self, desc: &str, count: u64) -> Result<()> {
        let delta = self.baseline.delta(self.fix);
        info!(
            "{}: residency = {}, instrs = {}, read_locks = {:.1}, write_locks = {:.1}",
            desc,
            self.residency()?,
            delta.instrs / count,
            delta.read_locks as f64 / count as f64,
            delta.write_locks as f64 / count as f64
        );
        Ok(())
    }

    fn residency(&self) -> Result<usize> {
        calc_residency(self.root)
    }
}

impl<'a> Drop for BTreeTest<'a> {
    fn drop(&mut self) {
        dm_bm_unlock(self.fix, self.sb).expect("unlock superblock");
        dm_tm_destroy(self.fix, self.tm).expect("destroy tm");
        dm_bm_destroy(self.fix, self.bm).expect("destroy bm");
    }
}

// keys contains the keys we wish to insert, in the order
// that they should be inserted.
fn do_insert_test_(
    fix: &mut Fixture,
    keys: &[u64],
    pass_count: usize,
    target_residency: usize,
) -> Result<()> {
    standard_globals(fix)?;
    let mut bt = BTreeTest::new(fix)?;
    let commit_interval = 100;

    // First pass inserts, subsequent passes overwrite
    let mut commit_counter = commit_interval;
    for pass in 0..pass_count {
        bt.stats_start();
        for k in keys {
            bt.insert(*k)?;

            if commit_counter == 0 {
                bt.commit()?;
                commit_counter = commit_interval;
            }
            commit_counter -= 1;
        }

        let residency = bt.residency()?;
        if residency < target_residency {
            // return Err(anyhow!("Residency is too low ({}%)", residency));
        }

        let desc = if pass == 0 { "insert" } else { "overwrite" };
        bt.stats_report(desc, keys.len() as u64)?;
    }

    bt.commit()?;

    // Lookup
    bt.stats_start();
    for k in keys {
        bt.lookup(*k)?;
    }
    bt.stats_report("lookup", keys.len() as u64)?;
    bt.commit()?;

    bt.check_keys_present(&keys)?;

    Ok(())
}

const KEY_COUNT: u64 = 10240;

fn test_insert_ascending(fix: &mut Fixture) -> Result<()> {
    let keys: Vec<u64> = (0..KEY_COUNT).collect();
    do_insert_test_(fix, &keys, 2, 75)
}

fn test_insert_descending(fix: &mut Fixture) -> Result<()> {
    let keys: Vec<u64> = (0..KEY_COUNT).rev().collect();
    do_insert_test_(fix, &keys, 2, 49)
}

fn test_insert_random(fix: &mut Fixture) -> Result<()> {
    let mut keys: Vec<u64> = (0..KEY_COUNT).rev().collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    keys.shuffle(&mut rng);
    do_insert_test_(fix, &keys, 2, 75)
}

fn test_insert_runs(fix: &mut Fixture) -> Result<()> {
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);

    let mut endpoints = BTreeSet::new();
    for _ in 0..500 {
        endpoints.insert(rng.gen_range(0..KEY_COUNT));
    }
    endpoints.insert(KEY_COUNT);

    let mut ranges = Vec::new();
    let mut last = 0;
    for e in endpoints {
        if e != last {
            ranges.push(last..e);
        }
        last = e;
    }
    ranges.shuffle(&mut rng);

    let mut shuffled_keys = Vec::new();
    for r in ranges {
        for k in r {
            shuffled_keys.push(k);
        }
    }

    do_insert_test_(fix, &shuffled_keys, 2, 80)
}

//-------------------------------

// comsume_cursor() tests
fn test_cc_empty_cursor_fails(fix: &mut Fixture) -> Result<()> {
    let mut cursor = CopyCursor {
        index: 0,
        entries: Vec::new(),
    };

    ensure!(consume_cursor(fix, &mut cursor, 1).is_err());
    Ok(())
}

fn test_cc_one_entry(fix: &mut Fixture) -> Result<()> {
    let mk_cursor = |begin, end| CursorEntry {
        node: Addr(0),
        begin,
        end,
    };

    let mut cursor = CopyCursor {
        index: 0,
        entries: vec![mk_cursor(0, 1024)],
    };

    let after = CopyCursor {
        index: 0,
        entries: vec![mk_cursor(16, 1024)],
    };

    consume_cursor(fix, &mut cursor, 16)?;
    ensure!(cursor == after);

    let after = CopyCursor {
        index: 0,
        entries: vec![mk_cursor(512, 1024)],
    };
    consume_cursor(fix, &mut cursor, 512 - 16)?;
    ensure!(cursor == after);

    let after = CopyCursor {
        index: 1,
        entries: vec![mk_cursor(512, 1024)],
    };
    consume_cursor(fix, &mut cursor, 512)?;
    ensure!(cursor == after);

    // There should be no more entries
    ensure!(consume_cursor(fix, &mut cursor, 1).is_err());

    Ok(())
}

fn test_cc_two_entries(fix: &mut Fixture) -> Result<()> {
    let mk_cursor = |begin, end| CursorEntry {
        node: Addr(0),
        begin,
        end,
    };

    let mut cursor = CopyCursor {
        index: 0,
        entries: vec![mk_cursor(0, 10), mk_cursor(34, 96)],
    };

    let after = CopyCursor {
        index: 1,
        entries: vec![mk_cursor(0, 10), mk_cursor(36, 96)],
    };

    consume_cursor(fix, &mut cursor, 12)?;
    ensure!(cursor == after);

    let after = CopyCursor {
        index: 1,
        entries: vec![mk_cursor(0, 10), mk_cursor(46, 96)],
    };

    consume_cursor(fix, &mut cursor, 10)?;
    ensure!(cursor == after);

    // Insufficient entries
    ensure!(consume_cursor(fix, &mut cursor, 100).is_err());

    Ok(())
}

fn test_cc_multiple_entries(fix: &mut Fixture) -> Result<()> {
    let mk_cursor = |begin, end| CursorEntry {
        node: Addr(0),
        begin,
        end,
    };

    let mut cursor = CopyCursor {
        index: 0,
        entries: vec![mk_cursor(0, 10), mk_cursor(34, 96), mk_cursor(17, 34)],
    };

    let after = CopyCursor {
        index: 2,
        entries: vec![mk_cursor(0, 10), mk_cursor(34, 96), mk_cursor(20, 34)],
    };

    consume_cursor(fix, &mut cursor, 10 + (96 - 34) + 3)?;
    ensure!(cursor == after);

    Ok(())
}

//-------------------------------

fn mk_node<'a>(fix: &'a mut Fixture, nr_entries: usize) -> Result<(AutoGPtr<'a>, Addr)> {
    let header = NodeHeader {
        block: 1,
        is_leaf: true,
        nr_entries: nr_entries as u32,
        max_entries: calc_max_entries::<Value64>() as u32,
        value_size: Value64::guest_len() as u32,
    };
    let keys: Vec<u64> = (0..nr_entries as u64).collect();
    let values: Vec<Value64> = (0..nr_entries as u64).map(Value64).collect();
    let node = Node::Leaf {
        header,
        keys,
        values,
    };

    let mut buffer = vec![0u8; BLOCK_SIZE];
    let mut w = Cursor::new(&mut buffer);
    pack_node(&node, &mut w)?;
    drop(w);

    let (mut fix, block) = auto_alloc(fix, BLOCK_SIZE)?;
    fix.vm.mem.write(block, &buffer, PERM_WRITE)?;

    Ok((fix, block))
}

#[derive(Debug, PartialEq, Eq)]
struct Move {
    dest: Addr,
    src: Addr,
    len: usize,
}

// This checks that we never read a region after writing it.  Since
// the src and dest copy_cursors overlap this is a real concern.
fn check_moves(moves: &[Move]) -> Result<()> {
    // Tracks which bytes have been written
    let mut writes = BTreeSet::new();

    info!("{:?}", moves);
    for m in moves {
        for i in 0..m.len {
            ensure!(!writes.contains(&(m.src.0 + i as u64)));
            writes.insert(m.dest.0 + i as u64);
        }
    }

    Ok(())
}

// We test redistribute_entries() by capturing a trace of the memmove
// calls it makes, and checking for out of order copies.
fn do_redistribute_test(
    fix: &mut Fixture,
    mut dest: CopyCursor,
    mut src: CopyCursor,
) -> Result<()> {
    let moves = Arc::new(Mutex::new(Vec::new()));

    // Register a stub for memmove that captures call details
    // but does nothing.
    {
        let moves = moves.clone();
        let memmove = move |fix: &mut Fixture| -> Result<()> {
            use Reg::*;
            let dest = Addr(fix.vm.reg(A0));
            let src = Addr(fix.vm.reg(A1));
            let len = fix.vm.reg(A2) as usize;
            let mut moves = moves.lock().unwrap();
            moves.push(Move { dest, src, len });
            fix.vm.ret(0);
            Ok(())
        };

        fix.at_func("memmove", Box::new(memmove))?;
    }

    redistribute_entries(&mut *fix, &mut dest, &mut src)?;

    let moves = moves.lock().unwrap();
    check_moves(&moves)?;

    Ok(())
}

fn do_redistribute_2(fix: &mut Fixture, lhs_count: u32, rhs_count: u32) -> Result<()> {
    let total_count = lhs_count + rhs_count;
    let lhs_target = total_count / 2;
    let rhs_target = total_count - lhs_target;

    let (mut fix, node1_ptr) = mk_node(fix, lhs_count as usize)?;
    let (mut fix, node2_ptr) = mk_node(&mut *fix, rhs_count as usize)?;

    let dest = CopyCursor {
        index: 0,
        entries: vec![
            CursorEntry::new(node1_ptr, 0, lhs_target),
            CursorEntry::new(node2_ptr, 0, rhs_target),
        ],
    };

    let src = CopyCursor {
        index: 0,
        entries: vec![
            CursorEntry::new(node1_ptr, 0, lhs_count),
            CursorEntry::new(node2_ptr, 0, rhs_count),
        ],
    };

    info!("dest: {:?}", dest);
    info!("src: {:?}", src);
    do_redistribute_test(&mut *fix, dest, src)
}

fn test_redistribute_entries(fix: &mut Fixture) -> Result<()> {
    standard_globals(fix)?;

    do_redistribute_2(fix, 0, 100)?;
    do_redistribute_2(fix, 25, 75)?;
    do_redistribute_2(fix, 50, 50)?;
    info!("75, 25");
    do_redistribute_2(fix, 75, 25)?;
    info!("100, 0");
    do_redistribute_2(fix, 100, 0)?;
    Ok(())
}

//-------------------------------

fn test_split_one_into_two_bad_redistribute(fix: &mut Fixture) -> Result<()> {
    standard_globals(fix)?;

    Ok(())
}

//-------------------------------

pub fn register_tests(runner: &mut TestRunner) -> Result<()> {
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
            runner.register(&p, Box::new($func));
        }};
    }

    test_section! {
        "/pdata/btree/",
        test!("del/empty", test_del_empty)

        test_section! {
            "insert-overtwrite-lookup/",
            test!("ascending", test_insert_ascending)
            test!("descending", test_insert_descending)
            test!("random", test_insert_random)
            test!("runs", test_insert_runs)
        }

        test_section! {
            "consume_cursor/",
            test!(
                "empty-cursor-fails",
                test_cc_empty_cursor_fails
            )
            test!("one-entry", test_cc_one_entry)
            test!("two-entries", test_cc_two_entries)
            test!("multiple-entries", test_cc_multiple_entries)
        }

        test!("redistribute-entries", test_redistribute_entries)
        test!(
            "split_one_into_two/bad-redistribute",
            test_split_one_into_two_bad_redistribute
        )
    };

    Ok(())
}

//-------------------------------
