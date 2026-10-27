//! Explicit link scopes. Dependency closure and observed process-global lookup
//! are separate policies; DT_NEEDED alone does not establish process interposition.
use crate::LoadedModule;
use mkw_formats::{Result, Symbol};
use std::collections::{BTreeMap, VecDeque};
#[derive(Debug, Clone)]
pub struct LinkScopes {
    pub dependencies: Vec<Vec<usize>>,
    pub search: Vec<Vec<usize>>,
    pub policy: &'static str,
}
fn soname(module: &LoadedModule) -> Result<Option<String>> {
    let Some(info) = &module.nso.mod0 else {
        return Ok(None);
    };
    let Some(offset) = info.tags.get(&14).and_then(|v| v.first()) else {
        return Ok(None);
    };
    let tag = |t| {
        info.tags
            .get(&t)
            .and_then(|v| v.first())
            .copied()
            .ok_or_else(|| format!("missing dynamic tag {t}"))
    };
    let size = tag(10)?;
    if *offset >= size {
        return Err("SONAME outside string table".into());
    }
    let base = tag(5)?.checked_add(*offset).ok_or("SONAME overflow")?;
    let tail = module.nso.read(
        usize::try_from(base).map_err(|_| "SONAME address overflow")?,
        (size - offset) as usize,
    )?;
    let end = tail
        .iter()
        .position(|b| *b == 0)
        .ok_or("unterminated SONAME")?;
    Ok(Some(
        std::str::from_utf8(&tail[..end])
            .map_err(|_| "non-UTF8 SONAME")?
            .to_string(),
    ))
}
impl LinkScopes {
    /// Dependencies bind to DT_SONAME only, never guessed filename aliases.
    pub fn dependencies(modules: &[LoadedModule]) -> Result<Self> {
        let mut names = BTreeMap::new();
        for (i, m) in modules.iter().enumerate() {
            if let Some(name) = soname(m)? {
                if names.insert(name.clone(), i).is_some() {
                    return Err(format!("duplicate DT_SONAME {name}"));
                }
            }
        }
        let mut dependencies = vec![Vec::new(); modules.len()];
        for (i, m) in modules.iter().enumerate() {
            if let Some(info) = &m.nso.mod0 {
                for name in &info.needed {
                    let &target = names
                        .get(name)
                        .ok_or_else(|| format!("{}: missing dependency {name}", m.name))?;
                    if !dependencies[i].contains(&target) {
                        dependencies[i].push(target)
                    }
                }
            }
        }
        let mut search = Vec::new();
        for root in 0..modules.len() {
            let mut seen = vec![false; modules.len()];
            let mut queue = VecDeque::from([root]);
            let mut scope = Vec::new();
            while let Some(i) = queue.pop_front() {
                if seen[i] {
                    continue;
                }
                seen[i] = true;
                scope.push(i);
                queue.extend(dependencies[i].iter().copied());
            }
            search.push(scope);
        }
        Ok(Self {
            dependencies,
            search,
            policy: "requester then breadth-first DT_NEEDED closure",
        })
    }
    pub fn process_order(modules: &[LoadedModule], order: &[usize]) -> Result<Self> {
        let mut scopes = Self::dependencies(modules)?;
        let mut seen = vec![false; modules.len()];
        if order.len() != modules.len() {
            return Err("process scope must enumerate every module".into());
        }
        for &i in order {
            if i >= modules.len() || seen[i] {
                return Err("invalid or duplicate process-scope module".into());
            }
            seen[i] = true;
        }
        scopes.search = vec![order.to_vec(); modules.len()];
        scopes.policy = "explicit process module order; first visible global/weak definition wins";
        Ok(scopes)
    }
}
pub(crate) struct Resolver<'a> {
    modules: &'a [LoadedModule],
    scopes: &'a LinkScopes,
    exports: Vec<BTreeMap<&'a str, &'a Symbol>>,
}
impl<'a> Resolver<'a> {
    pub fn new(modules: &'a [LoadedModule], scopes: &'a LinkScopes) -> Result<Self> {
        if scopes.search.len() != modules.len() {
            return Err("scope/module count mismatch".into());
        }
        let mut exports = Vec::new();
        for m in modules {
            let mut table: BTreeMap<&str, &Symbol> = BTreeMap::new();
            if let Some(info) = &m.nso.mod0 {
                for s in &info.symbols {
                    if s.name.is_empty()
                        || s.section == 0
                        || s.section == 0xfff2
                        || s.info & 15 == 4
                        || !matches!(s.info >> 4, 1 | 2)
                        || matches!(s.other & 3, 1 | 2)
                    {
                        continue;
                    }
                    if let Some(old) = table.insert(&s.name, s) {
                        if old.value != s.value || old.section != s.section {
                            return Err(format!(
                                "{}: duplicate definition requires version-aware lookup: {}",
                                m.name, s.name
                            ));
                        }
                    }
                }
            }
            exports.push(table);
        }
        for scope in &scopes.search {
            for &i in scope {
                if i >= modules.len() {
                    return Err("scope index outside modules".into());
                }
            }
        }
        Ok(Self {
            modules,
            scopes,
            exports,
        })
    }
    fn address(&self, module: usize, s: &Symbol) -> Result<u64> {
        if !matches!(s.info & 15, 0..=2) {
            return Err(format!(
                "unsupported symbol type {}: {}",
                s.info & 15,
                s.name
            ));
        }
        match s.section {
            0 => Err("undefined definition".into()),
            0xfff1 => Ok(s.value),
            0xff00..=0xffff => Err("unsupported reserved symbol section".into()),
            _ => self.modules[module]
                .base
                .0
                .checked_add(s.value)
                .ok_or_else(|| "symbol address overflow".into()),
        }
    }
    pub fn resolve(
        &self,
        requester: usize,
        s: &Symbol,
        index: u32,
    ) -> Result<(u64, Option<usize>)> {
        let binding = s.info >> 4;
        let visibility = s.other & 3;
        if !matches!(binding, 0..=2) {
            return Err(format!("unsupported symbol binding {binding}: {}", s.name));
        }
        if !matches!(s.info & 15, 0..=2) {
            return Err(format!(
                "unsupported symbol type {}: {}",
                s.info & 15,
                s.name
            ));
        }
        if s.section != 0 && (binding == 0 || visibility != 0 || s.section == 0xfff1) {
            return Ok((self.address(requester, s)?, Some(requester)));
        }
        if s.section == 0 && visibility != 0 {
            return Err(format!("undefined non-default-visible symbol: {}", s.name));
        }
        if s.name.is_empty() && index == 0 {
            return Ok((0, None));
        }
        if binding == 0 {
            return Err(format!("undefined local symbol: {}", s.name));
        }
        for &module in &self.scopes.search[requester] {
            if let Some(definition) = self.exports[module].get(s.name.as_str()) {
                return Ok((self.address(module, definition)?, Some(module)));
            }
        }
        if binding == 2 {
            return Ok((0, None));
        }
        Err(format!(
            "unresolved import {} in {} under {}",
            s.name, self.modules[requester].name, self.scopes.policy
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relocation_tests::module;
    use crate::{map_modules, plan_relocations_scoped, GuestAddr};
    use mkw_formats::{Relocation, Symbol};
    fn named(name: &str, base: u64, needed: &[&str]) -> LoadedModule {
        let mut m = module(name, base);
        let mut strings = name.as_bytes().to_vec();
        strings.push(0);
        m.nso.segments[1].size = strings.len();
        m.nso.segments[1].bytes = strings;
        let info = m.nso.mod0.as_mut().unwrap();
        info.tags.insert(5, vec![0x1000]);
        info.tags.insert(10, vec![(name.len() + 1) as u64]);
        info.tags.insert(14, vec![0]);
        info.needed = needed.iter().map(|s| s.to_string()).collect();
        m
    }
    fn definition(m: &mut LoadedModule, weak: bool, visibility: u8) {
        m.nso.mod0.as_mut().unwrap().symbols.push(Symbol {
            name: "f".into(),
            value: 4,
            size: 4,
            info: if weak { 0x22 } else { 0x12 },
            other: visibility,
            section: 1,
        });
    }
    #[test]
    fn dependency_breadth_first_cycles_and_missing() {
        let modules = [
            named("a", 0x10000, &["b", "c"]),
            named("b", 0x20000, &["d"]),
            named("c", 0x30000, &["d"]),
            named("d", 0x40000, &["a"]),
        ];
        assert_eq!(
            LinkScopes::dependencies(&modules).unwrap().search[0],
            vec![0, 1, 2, 3]
        );
        assert!(LinkScopes::dependencies(&modules[..3])
            .unwrap_err()
            .contains("missing dependency"));
        let duplicate = [named("b", 0x10000, &[]), named("b", 0x20000, &[])];
        assert!(LinkScopes::dependencies(&duplicate).is_err());
        assert!(LinkScopes::process_order(&modules, &[0, 0, 2, 3]).is_err());
    }
    #[test]
    fn unrelated_exports_cannot_leak_into_dependency_scope() {
        let a = named("a", 0x10000, &["b"]);
        let mut b = named("b", 0x20000, &[]);
        let mut unrelated = named("c", 0x30000, &[]);
        definition(&mut b, false, 0);
        definition(&mut unrelated, false, 0);
        let mut modules = [a, b, unrelated];
        let info = modules[0].nso.mod0.as_mut().unwrap();
        info.symbols.push(Symbol {
            name: "f".into(),
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        });
        info.relocations.push(Relocation {
            offset: 0x2000,
            kind: 257,
            symbol: 1,
            addend: 0,
        });
        let scopes = LinkScopes::dependencies(&modules).unwrap();
        let mut memory = map_modules(&modules).unwrap();
        let plan = plan_relocations_scoped(&modules, &memory, &scopes).unwrap();
        plan.apply(&mut memory).unwrap();
        plan.verify_applied(&memory).unwrap();
        assert_eq!(memory.read_u64(GuestAddr(0x12000)).unwrap(), 0x20004);
        memory.write(GuestAddr(0x12000), &[0; 8]).unwrap();
        assert!(plan.verify_applied(&memory).is_err());
        modules[1].nso.mod0.as_mut().unwrap().symbols[1].other = 2;
        assert!(plan_relocations_scoped(&modules, &memory, &scopes)
            .unwrap_err()
            .contains("unresolved import"));
    }
    #[test]
    fn process_order_keeps_first_weak_definition() {
        let mut a = named("a", 0x10000, &[]);
        let mut b = named("b", 0x20000, &[]);
        definition(&mut a, true, 0);
        definition(&mut b, false, 0);
        let modules = [a, b];
        let scopes = LinkScopes::process_order(&modules, &[0, 1]).unwrap();
        let r = Resolver::new(&modules, &scopes).unwrap();
        let s = Symbol {
            name: "f".into(),
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        };
        assert_eq!(r.resolve(1, &s, 1).unwrap(), (0x10004, Some(0)));
    }
}
