use super::control::reconverge_continuations;
use super::{Exit, ExitBuffer, Flow};

pub(super) enum WindowExits {
    Compact(Vec<WindowChunk>),
    Expanded(Vec<Exit>),
}

pub(super) enum WindowChunk {
    Single(Box<Exit>),
    Terminal(TerminalRun),
}

pub(super) struct TerminalRun {
    length: usize,
    tree: Box<TerminalTree>,
}

enum TerminalTree {
    Leaf(Box<Exit>),
    Concat {
        left: TerminalRun,
        right: TerminalRun,
    },
}

pub(super) enum WindowBuffer {
    Compact {
        chunks: Vec<WindowChunk>,
        length: usize,
    },
    Expanded(ExitBuffer),
}

impl TerminalRun {
    fn one(exit: Exit) -> Self {
        Self {
            length: 1,
            tree: Box::new(TerminalTree::Leaf(Box::new(exit))),
        }
    }

    fn concatenate(left: Self, right: Self) -> Self {
        Self {
            length: left.length + right.length,
            tree: Box::new(TerminalTree::Concat { left, right }),
        }
    }

    fn into_exits(self) -> Vec<Exit> {
        let mut exits = Vec::with_capacity(self.length);
        let mut pending = vec![self];
        while let Some(Self { length: _, tree }) = pending.pop() {
            match *tree {
                TerminalTree::Leaf(exit) => exits.push(*exit),
                TerminalTree::Concat { left, right } => pending.extend([right, left]),
            }
        }
        exits
    }
}

fn append(chunks: &mut Vec<WindowChunk>, chunk: WindowChunk) {
    match (chunks.pop(), chunk) {
        (Some(WindowChunk::Terminal(left)), WindowChunk::Terminal(right)) => {
            chunks.push(WindowChunk::Terminal(TerminalRun::concatenate(left, right)));
        }
        (last, chunk) => {
            chunks.extend(last);
            chunks.push(chunk);
        }
    }
}

impl WindowExits {
    pub(super) fn new(exits: Vec<Exit>) -> Self {
        let mut chunks = Vec::new();
        for exit in exits {
            let chunk = if matches!(exit.flow, Flow::Normal(_)) {
                WindowChunk::Single(Box::new(exit))
            } else {
                WindowChunk::Terminal(TerminalRun::one(exit))
            };
            append(&mut chunks, chunk);
        }
        Self::Compact(chunks)
    }

    pub(super) fn next_buffer(&self) -> WindowBuffer {
        match self {
            Self::Compact(_) => WindowBuffer::Compact {
                chunks: Vec::new(),
                length: 0,
            },
            Self::Expanded(_) => WindowBuffer::Expanded(ExitBuffer::default()),
        }
    }

    pub(super) fn emit_bytes(
        &mut self,
        span: helm_schema_syntax::Span,
        path: Option<&str>,
        bytes: &str,
    ) {
        match self {
            Self::Compact(chunks) => {
                for chunk in chunks {
                    if let WindowChunk::Single(exit) = chunk {
                        exit.state.emit_bytes(span, path, bytes);
                    }
                }
            }
            Self::Expanded(exits) => {
                for exit in exits {
                    if matches!(exit.flow, Flow::Normal(_)) {
                        exit.state.emit_bytes(span, path, bytes);
                    }
                }
            }
        }
    }

    pub(super) fn into_chunks(self) -> Vec<WindowChunk> {
        match self {
            Self::Compact(chunks) => chunks,
            Self::Expanded(exits) => exits
                .into_iter()
                .map(|exit| WindowChunk::Single(Box::new(exit)))
                .collect(),
        }
    }

    pub(super) fn into_exits(self) -> Vec<Exit> {
        self.into_chunks()
            .into_iter()
            .flat_map(|chunk| match chunk {
                WindowChunk::Single(exit) => vec![*exit],
                WindowChunk::Terminal(run) => run.into_exits(),
            })
            .collect()
    }
}

impl WindowBuffer {
    fn expand(&mut self) {
        let previous = std::mem::replace(self, Self::Expanded(ExitBuffer::default()));
        let buffer = match previous {
            Self::Expanded(buffer) => buffer,
            Self::Compact { chunks, length: _ } => {
                let mut buffer = ExitBuffer::default();
                buffer.extend(WindowExits::Compact(chunks).into_exits());
                buffer
            }
        };
        *self = Self::Expanded(buffer);
    }

    pub(super) fn forward(&mut self, run: TerminalRun) {
        if let Self::Compact { chunks, length } = self
            && *length + run.length <= 4096
        {
            *length += run.length;
            append(chunks, WindowChunk::Terminal(run));
            return;
        }
        // Replay at the original logical push, before evaluating the next live continuation.
        self.expand();
        if let Self::Expanded(buffer) = self {
            buffer.extend(run.into_exits());
        }
    }

    pub(super) fn push(&mut self, exit: Exit) {
        if let Self::Compact { chunks, length } = self
            && *length < 4096
        {
            *length += 1;
            append(
                chunks,
                if matches!(exit.flow, Flow::Normal(_)) {
                    WindowChunk::Single(Box::new(exit))
                } else {
                    WindowChunk::Terminal(TerminalRun::one(exit))
                },
            );
            return;
        }
        self.expand();
        if let Self::Expanded(buffer) = self {
            buffer.push(exit);
        }
    }

    pub(super) fn extend(&mut self, exits: impl IntoIterator<Item = Exit>) {
        for exit in exits {
            self.push(exit);
        }
    }

    pub(super) fn finish(self) -> WindowExits {
        match self {
            Self::Expanded(buffer) => WindowExits::Expanded(buffer.finish()),
            Self::Compact { chunks, length: _ } => {
                let mut normals = Vec::new();
                let mut slots = Vec::new();
                for chunk in chunks {
                    match chunk {
                        WindowChunk::Single(exit) => {
                            slots.push(WindowSlot::Normal(normals.len()));
                            normals.push(*exit);
                        }
                        WindowChunk::Terminal(run) => slots.push(WindowSlot::Terminal(run)),
                    }
                }
                let mut joined = reconverge_continuations(normals, &())
                    .into_iter()
                    .peekable();
                let mut chunks = Vec::new();
                for slot in slots {
                    match slot {
                        WindowSlot::Terminal(run) => {
                            append(&mut chunks, WindowChunk::Terminal(run));
                        }
                        WindowSlot::Normal(index) => {
                            if joined.peek().is_some_and(|entry| entry.index == index)
                                && let Some(entry) = joined.next()
                            {
                                append(
                                    &mut chunks,
                                    WindowChunk::Single(Box::new(entry.continuation)),
                                );
                            }
                        }
                    }
                }
                WindowExits::Compact(chunks)
            }
        }
    }
}

enum WindowSlot {
    Normal(usize),
    Terminal(TerminalRun),
}
