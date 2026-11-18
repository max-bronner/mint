const THRESHOLD: usize = 16;

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn lg(mut n: usize) -> usize {
    let mut k = 0;
    while n > 1 {
        n >>= 1;
        k += 1;
    }
    k
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
pub(crate) fn sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    let n = v.len();
    if n == 0 {
        return;
    }
    introsort_loop(v, 0, n, lg(n) * 2, less);
    final_insertion_sort(v, less);
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn median<T: Copy>(a: T, b: T, c: T, less: &impl Fn(&T, &T) -> bool) -> T {
    if less(&a, &b) {
        if less(&b, &c) {
            b
        } else if less(&a, &c) {
            c
        } else {
            a
        }
    } else if less(&a, &c) {
        a
    } else if less(&b, &c) {
        c
    } else {
        b
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn introsort_loop<T: Copy>(
    v: &mut [T],
    first: usize,
    mut last: usize,
    mut depth: usize,
    less: &impl Fn(&T, &T) -> bool,
) {
    while last - first > THRESHOLD {
        if depth == 0 {
            heap_sort(&mut v[first..last], less);
            return;
        }
        depth -= 1;
        let pivot = median(v[first], v[first + (last - first) / 2], v[last - 1], less);
        let cut = unguarded_partition(v, first, last, pivot, less);
        introsort_loop(v, cut, last, depth, less);
        last = cut;
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn unguarded_partition<T: Copy>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    pivot: T,
    less: &impl Fn(&T, &T) -> bool,
) -> usize {
    loop {
        while less(&v[first], &pivot) {
            first += 1;
        }
        last -= 1;
        while less(&pivot, &v[last]) {
            last -= 1;
        }
        if first >= last {
            return first;
        }
        v.swap(first, last);
        first += 1;
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn final_insertion_sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    let n = v.len();
    if n > THRESHOLD {
        insertion_sort(v, THRESHOLD, less);
        for i in THRESHOLD..n {
            unguarded_linear_insert(v, i, v[i], less);
        }
    } else {
        insertion_sort(v, n, less);
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn insertion_sort<T: Copy>(v: &mut [T], last: usize, less: &impl Fn(&T, &T) -> bool) {
    for i in 1..last {
        let val = v[i];
        if less(&val, &v[0]) {
            v.copy_within(0..i, 1);
            v[0] = val;
        } else {
            unguarded_linear_insert(v, i, val, less);
        }
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn unguarded_linear_insert<T: Copy>(
    v: &mut [T],
    mut last: usize,
    val: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    while last > 0 && less(&val, &v[last - 1]) {
        v[last] = v[last - 1];
        last -= 1;
    }
    v[last] = val;
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn heap_sort<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    let len = v.len();
    if len >= 2 {
        let mut parent = (len - 2) / 2;
        loop {
            let value = v[parent];
            adjust_heap(v, parent, len, value, less);
            if parent == 0 {
                break;
            }
            parent -= 1;
        }
    }
    let mut end = len;
    while end > 1 {
        pop_heap(&mut v[..end], less);
        end -= 1;
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn adjust_heap<T: Copy>(
    v: &mut [T],
    mut hole: usize,
    len: usize,
    value: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    let top = hole;
    let mut child = 2 * hole + 2;
    while child < len {
        if less(&v[child], &v[child - 1]) {
            child -= 1;
        }
        v[hole] = v[child];
        hole = child;
        child = 2 * (child + 1);
    }
    if child == len {
        v[hole] = v[child - 1];
        hole = child - 1;
    }
    sift_up(v, hole, top, value, less);
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn sift_up<T: Copy>(
    v: &mut [T],
    mut hole: usize,
    top: usize,
    value: T,
    less: &impl Fn(&T, &T) -> bool,
) {
    while hole > top {
        let parent = (hole - 1) / 2;
        if !less(&v[parent], &value) {
            break;
        }
        v[hole] = v[parent];
        hole = parent;
    }
    v[hole] = value;
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
pub(crate) fn push_heap<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    if let Some(last) = v.len().checked_sub(1) {
        let value = v[last];
        sift_up(v, last, 0, value, less);
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
pub(crate) fn pop_heap<T: Copy>(v: &mut [T], less: &impl Fn(&T, &T) -> bool) {
    if let Some(last) = v.len().checked_sub(1) {
        let value = v[last];
        v[last] = v[0];
        adjust_heap(v, 0, last, value, less);
    }
}

#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
pub(crate) fn merge_stable<T: Copy>(v: &mut [T], mid: usize, less: &impl Fn(&T, &T) -> bool) {
    let left: Vec<T> = v[..mid].to_vec();
    let right: Vec<T> = v[mid..].to_vec();
    let (mut i, mut j) = (0, 0);
    for slot in v.iter_mut() {
        if j < right.len() && (i >= left.len() || less(&right[j], &left[i])) {
            *slot = right[j];
            j += 1;
        } else {
            *slot = left[i];
            i += 1;
        }
    }
}
