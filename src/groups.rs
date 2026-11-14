//! Agrupamento da lista de chats: primeiro por categoria (agente do Omarchy, local, VPS),
//! depois por pasta. Lógica pura, sem o `Chat`, para ser testável.

/// Categoria de um chat na lista, na ordem em que aparecem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// Modo do `omarchy agent`, que só existe neste notebook (`Chat::with_place`).
    Omarchy,
    Local,
    Vps,
}

impl Category {
    pub fn of(auto: bool, remote: bool) -> Self {
        match (auto, remote) {
            (_, true) => Category::Vps,
            (true, false) => Category::Omarchy,
            (false, false) => Category::Local,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Omarchy => "agente omarchy",
            Category::Local => "local",
            Category::Vps => "vps",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DirGroup {
    pub dir: String,
    /// Índices dos chats em `App::chats`, na ordem de abertura.
    pub chats: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CategoryGroup {
    pub category: Category,
    pub dirs: Vec<DirGroup>,
}

/// Agrupa `(categoria, pasta)` de cada chat. As categorias seguem a ordem fixa de `Category`;
/// as pastas, a ordem em que apareceram (o primeiro chat aberto nela); os chats, a de abertura.
pub fn group(chats: &[(Category, String)]) -> Vec<CategoryGroup> {
    let mut groups: Vec<CategoryGroup> = Vec::new();
    for (index, (category, dir)) in chats.iter().enumerate() {
        let position = match groups.iter().position(|group| group.category == *category) {
            Some(position) => position,
            None => {
                groups.push(CategoryGroup { category: *category, dirs: Vec::new() });
                groups.len() - 1
            }
        };
        let dirs = &mut groups[position].dirs;
        match dirs.iter_mut().find(|group| group.dir == *dir) {
            Some(group) => group.chats.push(index),
            None => dirs.push(DirGroup { dir: dir.clone(), chats: vec![index] }),
        }
    }
    groups.sort_by_key(|group| group.category);
    groups
}

/// Índices na ordem em que aparecem na lista (para os atalhos de chat de cima / de baixo).
pub fn display_order(groups: &[CategoryGroup]) -> Vec<usize> {
    groups
        .iter()
        .flat_map(|group| group.dirs.iter())
        .flat_map(|dir| dir.chats.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(category: Category, dir: &str) -> (Category, String) {
        (category, dir.to_owned())
    }

    #[test]
    fn categoria_pelo_modo_e_pelo_host() {
        assert_eq!(Category::of(true, false), Category::Omarchy);
        assert_eq!(Category::of(true, true), Category::Vps);
        assert_eq!(Category::of(false, false), Category::Local);
        assert_eq!(Category::of(false, true), Category::Vps);
    }

    #[test]
    fn agrupa_por_categoria_e_pasta() {
        let chats = [
            chat(Category::Vps, "vps:~/app"),
            chat(Category::Local, "~/projects/chube"),
            chat(Category::Omarchy, "~"),
            chat(Category::Local, "~/notas"),
            chat(Category::Local, "~/projects/chube"),
        ];
        let groups = group(&chats);
        let categories: Vec<_> = groups.iter().map(|group| group.category).collect();
        assert_eq!(categories, [Category::Omarchy, Category::Local, Category::Vps]);
        assert_eq!(
            groups[1].dirs,
            [
                DirGroup { dir: "~/projects/chube".into(), chats: vec![1, 4] },
                DirGroup { dir: "~/notas".into(), chats: vec![3] },
            ]
        );
        assert_eq!(display_order(&groups), [2, 1, 4, 3, 0]);
    }

    #[test]
    fn lista_vazia() {
        assert!(group(&[]).is_empty());
        assert!(display_order(&[]).is_empty());
    }
}
