use crate::block::{EmbedPrelim, ItemContent, ItemPosition, ItemPtr, Prelim};
use crate::encoding::read::Error;
use crate::encoding::serde::from_any;
use crate::transaction::TransactionMut;
use crate::types::{
    event_keys, AsPrelim, Branch, BranchPtr, DefaultPrelim, Entries, EntryChange, In, Out, Path,
    RootRef, SharedRef, ToJson, TypeRef,
};
use crate::*;
use serde::de::DeserializeOwned;
use std::borrow::Borrow;
use std::cell::UnsafeCell;
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Collection used to store key-value entries in an unordered manner. Keys are always represented
/// as UTF-8 strings. Values can be any value type supported by Yrs: JSON-like primitives as well as
/// shared data types.
///
/// In terms of conflict resolution, [MapRef] uses logical last-write-wins principle, meaning the past
/// updates are automatically overridden and discarded by newer ones, while concurrent updates made
/// by different peers are resolved into a single value using document id seniority to establish
/// order.
///
/// # Example
///
/// ```rust
/// use yrs::{any, Doc, Map, MapPrelim, Transact};
/// use yrs::types::ToJson;
///
/// let doc = Doc::new();
/// let map = doc.get_or_insert_map("map");
/// let mut txn = doc.transact_mut();
///
/// // insert value
/// map.insert(&mut txn, "key1", "value1");
///
/// // insert nested shared type
/// let nested = map.insert(&mut txn, "key2", MapPrelim::from([("inner", "value2")]));
/// nested.insert(&mut txn, "inner2", 100);
///
/// assert_eq!(map.to_json(&txn), any!({
///   "key1": "value1",
///   "key2": {
///     "inner": "value2",
///     "inner2": 100
///   }
/// }));
///
/// // get value
/// assert_eq!(map.get(&txn, "key1"), Some("value1".into()));
///
/// // remove entry
/// map.remove(&mut txn, "key1");
/// assert_eq!(map.get(&txn, "key1"), None);
/// ```
#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct MapRef(BranchPtr);

impl RootRef for MapRef {
    fn type_ref() -> TypeRef {
        TypeRef::Map
    }
}
impl SharedRef for MapRef {}
impl Map for MapRef {}

impl DeepObservable for MapRef {}
impl Observable for MapRef {
    type Event = MapEvent;
}

impl ToJson for MapRef {
    fn to_json<T: ReadTxn>(&self, txn: &T) -> Any {
        let inner = self.0;
        let mut res = HashMap::new();
        for (key, item) in inner.map.iter() {
            if !item.is_deleted() {
                let last = item.content.get_last().unwrap_or(Out::Any(Any::Null));
                res.insert(key.to_string(), last.to_json(txn));
            }
        }
        Any::from(res)
    }
}

impl AsRef<Branch> for MapRef {
    fn as_ref(&self) -> &Branch {
        self.0.deref()
    }
}

impl Eq for MapRef {}
impl PartialEq for MapRef {
    fn eq(&self, other: &Self) -> bool {
        self.0.id() == other.0.id()
    }
}

impl TryFrom<ItemPtr> for MapRef {
    type Error = ItemPtr;

    fn try_from(value: ItemPtr) -> Result<Self, Self::Error> {
        if let Some(branch) = value.clone().as_branch() {
            Ok(MapRef::from(branch))
        } else {
            Err(value)
        }
    }
}

impl TryFrom<Out> for MapRef {
    type Error = Out;

    fn try_from(value: Out) -> Result<Self, Self::Error> {
        match value {
            Out::YMap(value) => Ok(value),
            other => Err(other),
        }
    }
}

impl AsPrelim for MapRef {
    type Prelim = MapPrelim;

    fn as_prelim<T: ReadTxn>(&self, txn: &T) -> Self::Prelim {
        let mut prelim = HashMap::with_capacity(self.len(txn) as usize);
        for (key, &ptr) in self.0.map.iter() {
            if !ptr.is_deleted() {
                if let Ok(value) = Out::try_from(ptr) {
                    prelim.insert(key.clone(), value.as_prelim(txn));
                }
            }
        }
        MapPrelim(prelim)
    }
}

impl DefaultPrelim for MapRef {
    type Prelim = MapPrelim;

    #[inline]
    fn default_prelim() -> Self::Prelim {
        MapPrelim::default()
    }
}

pub trait Map: AsRef<Branch> + Sized {
    /// Returns a number of entries stored within current map.
    fn len<T: ReadTxn>(&self, _txn: &T) -> u32 {
        let mut len = 0;
        let inner = self.as_ref();
        for item in inner.map.values() {
            //TODO: maybe it would be better to just cache len in the map itself?
            if !item.is_deleted() {
                len += 1;
            }
        }
        len
    }

    /// Returns an iterator that enables to traverse over all keys of entries stored within
    /// current map. These keys are not ordered.
    fn keys<'a, T: ReadTxn + 'a>(&'a self, txn: &'a T) -> Keys<'a, &'a T, T> {
        Keys::new(self.as_ref(), txn)
    }

    /// Returns an iterator that enables to traverse over all values stored within current map.
    fn values<'a, T: ReadTxn + 'a>(&'a self, txn: &'a T) -> Values<'a, &'a T, T> {
        Values::new(self.as_ref(), txn)
    }

    /// Returns an iterator that enables to traverse over all entries - tuple of key-value pairs -
    /// stored within current map.
    fn iter<'a, T: ReadTxn + 'a>(&'a self, txn: &'a T) -> MapIter<'a, &'a T, T> {
        MapIter::new(self.as_ref(), txn)
    }

    fn into_iter<'a, T: ReadTxn + 'a>(self, txn: &'a T) -> MapIntoIter<'a, T> {
        let branch_ptr = BranchPtr::from(self.as_ref());
        MapIntoIter::new(branch_ptr, txn)
    }

    /// Inserts a new `value` under given `key` into current map. Returns an integrated value.
    fn insert<K, V>(&self, txn: &mut TransactionMut, key: K, value: V) -> V::Return
    where
        K: Into<Arc<str>>,
        V: Prelim,
    {
        let key = key.into();
        let pos = {
            let inner = self.as_ref();
            let left = inner.map.get(&key);
            ItemPosition {
                parent: BranchPtr::from(inner).into(),
                left: left.cloned(),
                right: None,
                index: 0,
                current_attrs: None,
            }
        };

        let ptr = txn
            .create_item(&pos, value, Some(key))
            .expect("Cannot insert empty value");
        if let Ok(integrated) = ptr.try_into() {
            integrated
        } else {
            panic!("Defect: unexpected integrated type")
        }
    }

    /// Tries to update a value stored under a given `key` within current map, if it's different
    /// from the current one. Returns `true` if the value was updated, `false` otherwise.
    ///
    /// The main difference from [Map::insert] is that this method will not insert a new value if
    /// it's the same as the current one. It's important distinction when dealing with shared types,
    /// as inserting an element will force previous value to be tombstoned, causing minimal memory
    /// overhead.
    ///
    /// # Example
    ///
    /// ```rust
    /// use yrs::{Doc, Map, Transact, WriteTxn};
    ///
    /// let doc = Doc::new();
    /// let mut txn = doc.transact_mut();
    /// let map = txn.get_or_insert_map("map");
    ///
    /// assert!(map.try_update(&mut txn, "key", 1)); // created a new entry
    /// assert!(!map.try_update(&mut txn, "key", 1)); // unchanged value doesn't trigger inserts...
    /// assert!(map.try_update(&mut txn, "key", 2)); // ... but changed one does
    /// ```
    fn try_update<K, V>(&self, txn: &mut TransactionMut, key: K, value: V) -> bool
    where
        K: Into<Arc<str>>,
        V: Into<Any>,
    {
        let key = key.into();
        let value = value.into();
        let branch = self.as_ref();
        if let Some(item) = branch.map.get(&key) {
            if !item.is_deleted() {
                if let ItemContent::Any(content) = &item.content {
                    if let Some(last) = content.last() {
                        if last == &value {
                            return false;
                        }
                    }
                }
            }
        }

        self.insert(txn, key, value);
        true
    }

    /// Returns an existing instance of a type stored under a given `key` within current map.
    /// If the given entry was not found, has been deleted or its type is different from expected,
    /// that entry will be reset to a given type and its reference will be returned.
    fn get_or_init<K, V>(&self, txn: &mut TransactionMut, key: K) -> V
    where
        K: Into<Arc<str>>,
        V: DefaultPrelim + TryFrom<Out>,
    {
        let key = key.into();
        let branch = self.as_ref();
        if let Some(value) = branch.get(txn, &key) {
            if let Ok(value) = value.try_into() {
                return value;
            }
        }
        let value = V::default_prelim();
        self.insert(txn, key, value)
    }

    /// Removes a stored within current map under a given `key`. Returns that value or `None` if
    /// no entry with a given `key` was present in current map.
    ///
    /// ### Removing nested shared types
    ///
    /// In case when a nested shared type (eg. [MapRef], [ArrayRef], [TextRef]) is being removed,
    /// all of its contents will also be deleted recursively. A returned value will contain a
    /// reference to a current removed shared type (which will be empty due to all of its elements
    /// being deleted), **not** the content prior the removal.
    fn remove(&self, txn: &mut TransactionMut, key: &str) -> Option<Out> {
        let ptr = BranchPtr::from(self.as_ref());
        ptr.remove(txn, key)
    }

    /// Returns [WeakPrelim] to a given `key`, if it exists in a current map.
    #[cfg(feature = "weak")]
    fn link<T: ReadTxn>(&self, _txn: &T, key: &str) -> Option<crate::WeakPrelim<Self>> {
        let ptr = BranchPtr::from(self.as_ref());
        let block = ptr.map.get(key)?;
        let start = StickyIndex::from_id(block.id().clone(), Assoc::Before);
        let end = StickyIndex::from_id(block.id().clone(), Assoc::After);
        let link = crate::WeakPrelim::new(start, end);
        Some(link)
    }

    /// Returns a value stored under a given `key` within current map, or `None` if no entry
    /// with such `key` existed.
    fn get<T: ReadTxn>(&self, txn: &T, key: &str) -> Option<Out> {
        let ptr = BranchPtr::from(self.as_ref());
        ptr.get(txn, key)
    }

    /// Returns a value stored under a given `key` within current map, deserializing it into expected
    /// type if found. If value was not found, the `Any::Null` will be substituted and deserialized
    /// instead (i.e. into instance of `Option` type, if so desired).
    ///
    /// # Example
    ///
    /// ```rust
    /// use yrs::{Doc, In, Map, MapPrelim, Transact, WriteTxn};
    ///
    /// let doc = Doc::new();
    /// let mut txn = doc.transact_mut();
    /// let map = txn.get_or_insert_map("map");
    ///
    /// // insert a multi-nested shared refs
    /// let alice = map.insert(&mut txn, "Alice", MapPrelim::from([
    ///   ("name", In::from("Alice")),
    ///   ("age", In::from(30)),
    ///   ("address", MapPrelim::from([
    ///     ("city", In::from("London")),
    ///     ("street", In::from("Baker st.")),
    ///   ]).into())
    /// ]));
    ///
    /// // define Rust types to map from the shared refs
    ///
    /// #[derive(Debug, PartialEq, serde::Deserialize)]
    /// struct Person {
    ///   name: String,
    ///   age: u32,
    ///   address: Option<Address>,
    /// }
    ///
    /// #[derive(Debug, PartialEq, serde::Deserialize)]
    /// struct Address {
    ///   city: String,
    ///   street: String,
    /// }
    ///
    /// // retrieve and deserialize the value across multiple shared refs
    /// let alice: Person = map.get_as(&txn, "Alice").unwrap();
    /// assert_eq!(alice, Person {
    ///   name: "Alice".to_string(),
    ///   age: 30,
    ///   address: Some(Address {
    ///     city: "London".to_string(),
    ///     street: "Baker st.".to_string(),
    ///   })
    /// });
    ///
    /// // try to retrieve value that doesn't exist
    /// let bob: Option<Person> = map.get_as(&txn, "Bob").unwrap();
    /// assert_eq!(bob, None);
    /// ```
    fn get_as<T, V>(&self, txn: &T, key: &str) -> Result<V, Error>
    where
        T: ReadTxn,
        V: DeserializeOwned,
    {
        let ptr = BranchPtr::from(self.as_ref());
        let out = ptr.get(txn, key).unwrap_or(Out::Any(Any::Null));
        //TODO: we could probably optimize this step by not serializing to intermediate Any value
        let any = out.to_json(txn);
        from_any(&any)
    }

    /// Checks if an entry with given `key` can be found within current map.
    fn contains_key<T: ReadTxn>(&self, _txn: &T, key: &str) -> bool {
        if let Some(item) = self.as_ref().map.get(key) {
            !item.is_deleted()
        } else {
            false
        }
    }

    /// Clears the contents of current map, effectively removing all of its entries.
    fn clear(&self, txn: &mut TransactionMut) {
        for (_, ptr) in self.as_ref().map.iter() {
            txn.delete(ptr.clone());
        }
    }
}

pub struct MapIter<'a, B, T>(Entries<'a, B, T>);

impl<'a, B, T> MapIter<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    pub fn new(branch: &'a Branch, txn: B) -> Self {
        let entries = Entries::new(&branch.map, txn);
        MapIter(entries)
    }
}

impl<'a, B, T> Iterator for MapIter<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    type Item = (&'a str, Out);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, item) = self.0.next()?;
        if let Some(content) = item.content.get_last() {
            Some((key, content))
        } else {
            self.next()
        }
    }
}

pub struct MapIntoIter<'a, T> {
    _txn: &'a T,
    entries: std::collections::hash_map::IntoIter<Arc<str>, ItemPtr>,
}

impl<'a, T: ReadTxn> MapIntoIter<'a, T> {
    fn new(map: BranchPtr, txn: &'a T) -> Self {
        let entries = map.map.clone().into_iter();
        MapIntoIter { _txn: txn, entries }
    }
}

impl<'a, T: ReadTxn> Iterator for MapIntoIter<'a, T> {
    type Item = (Arc<str>, Out);

    fn next(&mut self) -> Option<Self::Item> {
        let (key, item) = self.entries.next()?;
        if let Some(content) = item.content.get_last() {
            Some((key, content))
        } else {
            self.next()
        }
    }
}

/// An unordered iterator over the keys of a [Map].
#[derive(Debug)]
pub struct Keys<'a, B, T>(Entries<'a, B, T>);

impl<'a, B, T> Keys<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    pub fn new(branch: &'a Branch, txn: B) -> Self {
        let entries = Entries::new(&branch.map, txn);
        Keys(entries)
    }
}

impl<'a, B, T> Iterator for Keys<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, _) = self.0.next()?;
        Some(key)
    }
}

/// Iterator over the values of a [Map].
#[derive(Debug)]
pub struct Values<'a, B, T>(Entries<'a, B, T>);

impl<'a, B, T> Values<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    pub fn new(branch: &'a Branch, txn: B) -> Self {
        let entries = Entries::new(&branch.map, txn);
        Values(entries)
    }
}

impl<'a, B, T> Iterator for Values<'a, B, T>
where
    B: Borrow<T>,
    T: ReadTxn,
{
    type Item = Vec<Out>;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, item) = self.0.next()?;
        let len = item.len() as usize;
        let mut values = vec![Out::default(); len];
        if item.content.read(0, &mut values) == len {
            Some(values)
        } else {
            panic!("Defect: iterator didn't read all elements")
        }
    }
}

impl From<BranchPtr> for MapRef {
    fn from(inner: BranchPtr) -> Self {
        MapRef(inner)
    }
}

/// A preliminary map. It can be used to early initialize the contents of a [MapRef], when it's about
/// to be inserted into another Yrs collection, such as [ArrayRef] or another [MapRef].
#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapPrelim(HashMap<Arc<str>, In>);

impl Deref for MapPrelim {
    type Target = HashMap<Arc<str>, In>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MapPrelim {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<MapPrelim> for In {
    #[inline]
    fn from(value: MapPrelim) -> Self {
        In::Map(value)
    }
}

impl<S, T> FromIterator<(S, T)> for MapPrelim
where
    S: Into<Arc<str>>,
    T: Into<In>,
{
    fn from_iter<I: IntoIterator<Item = (S, T)>>(iter: I) -> Self {
        MapPrelim(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl<S, T, const C: usize> From<[(S, T); C]> for MapPrelim
where
    S: Into<Arc<str>>,
    T: Into<In>,
{
    fn from(map: [(S, T); C]) -> Self {
        let mut m = HashMap::with_capacity(C);
        for (key, value) in map {
            m.insert(key.into(), value.into());
        }
        MapPrelim(m)
    }
}

impl Prelim for MapPrelim {
    type Return = MapRef;

    fn into_content(self, _txn: &mut TransactionMut) -> (ItemContent, Option<Self>) {
        let inner = Branch::new(TypeRef::Map);
        (ItemContent::Type(inner), Some(self))
    }

    fn integrate(self, txn: &mut TransactionMut, inner_ref: BranchPtr) {
        let map = MapRef::from(inner_ref);
        for (key, value) in self.0 {
            map.insert(txn, key, value);
        }
    }
}

impl Into<EmbedPrelim<MapPrelim>> for MapPrelim {
    #[inline]
    fn into(self) -> EmbedPrelim<MapPrelim> {
        EmbedPrelim::Shared(self)
    }
}

/// Event generated by [Map::observe] method. Emitted during transaction commit phase.
pub struct MapEvent {
    pub(crate) current_target: BranchPtr,
    target: MapRef,
    keys: UnsafeCell<Result<HashMap<Arc<str>, EntryChange>, HashSet<Option<Arc<str>>>>>,
}

impl MapEvent {
    pub(crate) fn new(branch_ref: BranchPtr, key_changes: HashSet<Option<Arc<str>>>) -> Self {
        let current_target = branch_ref.clone();
        MapEvent {
            target: MapRef::from(branch_ref),
            current_target,
            keys: UnsafeCell::new(Err(key_changes)),
        }
    }

    /// Returns a [Map] instance which emitted this event.
    pub fn target(&self) -> &MapRef {
        &self.target
    }

    /// Returns a path from root type down to [Map] instance which emitted this event.
    pub fn path(&self) -> Path {
        Branch::path(self.current_target, self.target.0)
    }

    /// Returns a summary of key-value changes made over corresponding [Map] collection within
    /// bounds of current transaction.
    pub fn keys(&self, txn: &TransactionMut) -> &HashMap<Arc<str>, EntryChange> {
        let keys = unsafe { self.keys.get().as_mut().unwrap() };

        match keys {
            Ok(keys) => {
                return keys;
            }
            Err(subs) => {
                let subs = event_keys(txn, self.target.0, subs);
                *keys = Ok(subs);
                if let Ok(keys) = keys {
                    keys
                } else {
                    panic!("Defect: should not happen");
                }
            }
        }
    }
}
