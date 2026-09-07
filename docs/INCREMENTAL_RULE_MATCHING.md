# 目录级增量规则匹配设计

状态：设计提案，尚未实现或做性能验证。本文不改变现有规则语义。

## 结论与范围

可以把全路径匹配改为沿目录树推进的匹配状态：访问子节点时只输入它的名称，
保存“匹配完父路径后，还可能匹配哪些规则、匹配到了哪里”。重复访问节点时，
复用路径匹配结果；`.git` 等存在性条件单独维护，不重新跑 glob。

建议采用一个不可变规则程序、多棵按需建立的目录项树，以及独立的外部事实层。
节点保存父节点和单个名称，不保存完整路径；规则程序与匹配状态可以共享。
不要让每个节点复制规则对象，也不要把父目录的最终权限当成子目录的权限。

这能消除匹配器对完整路径的依赖，但不能单靠替换 globset 就删除 MirrorFs 的
路径表。宿主操作、符号链接解析、锁 broker 和日志目前都使用路径，需要另一步
迁移到目录项 ID，并按需构造路径，之后才考虑目录 fd 相对操作。

本次先保留为设计：路径 glob 的现有行为、特殊条件参数和硬链接授权存在需要
显式处理的边界。此时直接写一个仅支持普通 `.git` 场景的库容易成为另一套语义。
下一步应先抽取现有解析器及纯评估器，建立差分测试，再实现增量后端。

## 当前实现与必须保留的行为

以下以源码为兼容基线；[规则指南](RULES.md)仍是用户语法说明。发现二者不一致时，
应另行修订语义和文档，不在优化中悄悄修正。

| 位置 | 当前行为 | 对新实现的要求 |
| --- | --- | --- |
| `src/profile.rs::explicit_globs_for_rule` | 通常同时编译 `pattern` 和 `pattern/**`；以 `**` 结尾时不追加 | 不把规则当成仅匹配一个目录项 |
| `build_internal_rule_index` | 每条规则先插入隐式祖先条目，再插入显式条目；全局按源顺序 | 保存内部条目 ID、原规则 ID 和类型，不仅保存规则集合 |
| `visibility_with_runtime` | 较早的隐式祖先匹配可以改变后续 hide/deny 的结果；遇到显式结果即返回 | 不能用“最具体路径优先”或全局 deny 优先 |
| `ProfileController::check` | 隐式祖先还检查 `is_dir`；当前目录可通过读及写操作判定 | 不把它简化成只读遍历许可；操作到 errno 的映射保持原样 |
| `ancestor_has` | 从访问路径的 parent 开始，一直检查到 `/` | `/repo/.git` 不会让 `/repo` 本身满足条件，但会影响 `/repo/src` |
| `RealFsCheck` | 使用 `Path::exists`、`Path::is_dir`，跟随符号链接 | `.git` 文件也算；悬空链接不算；不是检查“是否为 git 仓库” |
| `AncestorHasCache` | 默认 1 秒 TTL，正值可供后代复用；负值只对查询的精确 parent 复用 | 增量版需定义事实时效，而非只监听自己看到的创建操作 |
| `should_cache_readdir` | 根据调用者条件和隐式祖先另有缓存策略 | 节点的路径状态可共享，目录列表和最终授权不能无条件共享 |
| `MirrorFs::remap_paths_after_rename` | 扫描路径表，删除目标路径及后代映射，再重写源子树路径 | 目录树能避免重写路径字符串，但仍需处理目标替换和状态失效 |

特别注意三个兼容性缺口：

1. 路径索引用 `Glob::new`，而 `exe` 使用
   `GlobBuilder::literal_separator(true)`。不能按指南中“`*` 不跨 `/`”的说明，
   直接把路径 glob 拆成组件；必须测试当前锁定依赖的真实行为。
2. `expand_pattern` 除绝对路径外还接受 `**/` 开头的模式。
   `ancestor_globs_for_rule` 是现有字符串/组件生成算法，不是精确的
   “存在某个可见后代”求解器。兼容后端应消费它实际生成的模式。
3. `ancestor-has=` 参数目前没有限制为单个 basename，也未拒绝空值、`..`、
   含 `/` 或绝对路径的值。现有语义是 `ancestor.join(value)` 后调用 exists。
   这些条件不能全部用“目录下的特殊名字”更新算法替代。

`exe` 与 `env` 仍按请求惰性读取；条件保持原顺序和短路行为。某条规则的条件失败，
不能从节点永久删除这条规则。`os.id`、include、HOME 和 PATH 解析仍属于加载阶段。

## 三层结构

### 1. 不可变规则程序

`Program` 包含展开后的有序规则、内部匹配条目、条件、路径自动机和报告来源信息。
每次加载生成唯一 `ProgramId`；规则 ID 仅在该代内有效。多个 tree 共享同一程序。
`exe` 的匹配器可以继续使用 globset，因为它不是沿被访问目录树增量推进的对象。

路径状态有两个不同概念：

- `continuation`：读取当前路径后仍可继续匹配的自动机状态；用于生成子节点状态。
- `accepted_entries`：当前路径结束时接受的内部条目 ID，按原顺序排列；用于授权。

只保存当前接受的规则不够。例如 `/a/b/c rw` 在 `/a` 尚未显式匹配，但必须在
`b/c` 继续尝试。对 `**` 的循环状态也不能因当前节点不接受而删除。

### 2. 目录项树

建议的概念数据结构（不是已承诺的 Rust API）：

```text
Entry {
    id: generational EntryId,
    parent: Option<EntryId>,
    name: Unix filename bytes,
    object: optional adapter ObjectId,
    children: name -> EntryId,       // 只记录已发现的节点
    path_state: StateId,
    accepted_entries: shared EntrySetId,
    local_facts: FactKey -> observed value + validity,
    inherited_facts: shared FactStateId,
    versions: program / topology / parent-state / local-facts
}
```

Entry 是命名空间中的一条目录项，不是宿主 inode。文件和目录都需要路径匹配状态；
只有目录需要孩子索引及本地 marker facts。创建前可使用 parent + name 构造临时
候选状态，授权成功且宿主创建成功后才注册目录项。

Unix 文件名必须保留原始字节，不经过 `to_string_lossy`。绝对根 `/` 是明确的初始
输入；对根的直接孩子不重复拼接 `/`。虚拟挂载根需要真实的策略路径前缀和祖先
facts，不能擅自作为“没有祖先”的 `/`。仅凭路径的 CLI 查询可从根推进各组件，
不必将整条查询路径永久插入树。

以 `(ProgramId, continuation)` 驻留共享状态，按容量限制缓存
`(StateId, child_name) -> StateId`。名称缓存必须有界，否则大量一次性文件名会
抵消省下来的路径内存。节点通过 parent 链引用祖先；FORGET 只有在引用、孩子和
打开句柄均不再需要时才可回收，ID 复用要带 generation。

### 3. 外部事实与请求评估

纯核心不做 stat、读 `/proc`、读取时钟或监听文件系统。调用方提供明确的事实和
请求上下文，包括目标 `is_dir`、调用者条件，以及 marker 的存在性和有效期。

路径集合可以长期缓存；最终 AccessDecision 不保存在共享节点里。
评估依次处理 accepted entries，复用当前请求内同一规则的条件结果，并保持
现有 Visibility 和读写错误码转换。报告接口返回原规则与内部匹配原因。

## 如何沿目录推进 glob

首选方向是字节级 NFA 的残余状态，而不是直接 `split('/')` 后匹配组件。
这样既能在组件边界保存状态，也能表达可能跨分隔符的 `*`、`**`、转义、字符类和
花括号分支。目录级增量指状态的保存粒度，不要求自动机字母表必须是目录名。

```text
root = advance(start, b"/")
child_continuation = advance(parent.continuation, separator + name)
child_accepted = finish_on_copy(child_continuation)
```

`finish_on_copy` 的路径结束判断不能消耗 continuation；否则接受当前目录后便
无法继续匹配其孩子。所有内部模式以各自 EntryId 标记接受状态，合并后输出稳定
排序的 ID；同一原规则多个模式接受时仍保持现有条件缓存和顺序。

不要自行写一个只覆盖常用模式的 glob parser。实现前需审查锁定版本 globset
能否导出适合编译的模式表示，以及所选自动机后端对字节模式、锚点、结束断言、
多模式输出和路径规范化的支持。若需要阅读第三方实现，按项目约定克隆到
`~/src/3p/`；本文没有声称某个外部 API 已经支持这些操作。

最稳妥的原型顺序：先让同一个规则前端产生现有内部 globs，旧后端做 oracle；
新后端只替换这些 globs 的执行器。完整语法覆盖未证明前，未支持模式必须走显式
标记的兼容后端，按需还原路径交给 globset，不能静默给出不匹配。若保留这种
fallback，就不能宣称整个匹配流程完全不构造路径。

可以先采用“最长确定的字面量前缀 trie + 原 globset 校验”降低候选数量，但它
只是过渡方案：含跨目录通配符的规则仍可能到处活跃，也不能直接消除全路径输入。
不要把它的性能推断成完整增量自动机的性能。

## `.git` 等 marker 的增量传播

对于经过分类确认的单组件 marker `m`，定义：

```text
local(d,m) = exists(d / m)
above(root,m) = false                   // 策略路径确为 / 时
above(child,m) = above(parent,m) OR local(parent,m)
ancestor_has(entry,m) = above(entry,m)
```

因此 `.git` 改变时，改变的是所在目录孩子及后代的 above，通常不是该目录自身的
above。多个嵌套仓库需要各自的 local 值；删掉外层 marker 后，内层仓库仍为 true。
不使用一个“仓库 root ID”来代替此递推，也不因某规则目前条件为 false 而删除规则。

facts 使用 `Present / Absent / Unknown` 加有效性信息。OR 中任一有效 Present
足以确定 true；没有 Present 且有 Unknown 则需要补充事实。Unknown 不能直接
当 false，因为跳过前面的 deny 规则可能落入后面的 rw。评估返回 `NeedFacts`，
由适配层补齐后重试；错误如何映射为现有 exists=false 或保守错误，必须是显式
模式，不能混为兼容行为。已过期的 Present 也不能继续用于放行。

推荐首版用单写者、显式子树 dirty 标记：更新目录 d 的 local 后，将已缓存孩子的
继承状态标脏；查询时先修复祖先，再修复自身。也可立即重算并在传给孩子的完整
状态（值、未知依赖和有效性）不变时停止。仅比较布尔值不够：旧 true 的证据和
有效期可能已经不同。不能只增加父节点版本而查询时仅检查未刷新的直接父节点，
否则深层缓存可能看不到变化。

| 宿主成功事件 | 需要更新的内容 |
| --- | --- |
| create/mkdir/link/symlink 一个被监测名字 | 重查所在目录的 local，不以事件类型直接推断 exists |
| unlink/rmdir 一个被监测名字 | 重查 local，传播继承状态变化 |
| rename 或覆盖 | 作为一个事务更新旧父和新父 facts；更新被移动子树路径状态 |
| `.git` 文件内容写入 | 普通文件存在性未变，不必传播；这里不解析 gitdir 内容 |
| 符号链接目标变化 | 对应 marker 事实可能变化，即使 `.git` 目录项本身没有事件 |
| profile reload | 换 ProgramId，重建所需 FactKey 与路径状态；旧代结果不供新代使用 |

对含路径、空值等 legacy `ancestor-has` 参数，首版保留独立的外部 probe。
未来可引入规范语法版本只允许 basename，但必须明确迁移和拒绝规则，不能优化时
直接收紧现有解析器。多个条件对每个 FactKey 分别计算，不预设只有 `.git`。

## 外部变化、一致性与多棵树

宿主可以绕过 FUSE 创建或删除 `.git`，符号链接目标也可能在另一棵树内。
所以“仅根据 FUSE 回调更新”不能维持当前会周期复查的行为。首次访问没列举过的
目录时，缺少 marker 记录表示 Unknown，不能表示 Absent。

首版建议保留最长 1 秒的事实有效期，注入单调时间，并对本进程成功的 mutation
立即失效。继承结论的有效期不得晚于所依赖事实；过期后在授权前重新取证。
这是有界陈旧模式，不是即时撤权保证。监听器可作为加速提示，但监听丢失、溢出、
重连或覆盖不到的目标，都需要失效及重新探测；不能把“收到事件”作为正确性的
唯一基础。严格外部一致性需要额外的宿主访问约束，纯匹配库无法提供。

多树的静态 Program 与 StateId 可共享，路径和祖先 facts 不可盲目共享。适配层可
建立 `host directory identity -> (TreeId, EntryId)` 反向索引，把一次变化广播给
已知别名；未知别名由有效期兜底。rename/mount/symlink 会改变这个关系。首版允许
对所有相关树保守失效，不必立刻实现精确订阅图。

一次请求必须使用同一 Program 和一致的树/facts 快照。首版用串行更新最容易审查；
以后并发读者使用版本校验重试或不可变快照。先依据变更前状态授权 rename 两端，
宿主操作成功后，在下一个相关授权前发布完整树变更；失败不移动节点。
这不会自动消除宿主系统调用之间的 TOCTOU，不能把缓存一致性声称为原子文件访问。
FUSE 内核已有 dentry/attr/readdir 缓存也需按适配层策略失效，纯核心只能报告变化。

## rename、硬链接和打开句柄

目录移动时，重挂 parent/name，不重写后代字符串；新前缀可能影响后代任意一条
绝对路径规则，因此必须将整个已缓存子树标脏。首版显式遍历标脏是 O(S)，其中 S
是已缓存子树大小；按需重算可推迟成本，但不是无条件 O(1) rename。
目标覆盖先分离目标目录项；旧打开对象的生命期不能被新目录项冒用。交换重命名
只有适配层支持后才扩展成双子树事务，不能当作两次独立普通 rename。

同一宿主 inode 的多个路径可能有不同权限。当前 `link_for_test` 会把新路径挂到
同一个 FUSE inode，`path_for_ino` 再选一个路径，这使 inode-only 操作存在别名
选择问题。新设计必须把 namespace Entry 和打开的 Object/Handle 分开。建议探索
每个目录项使用独立 FUSE node ID，句柄固定关联打开时的 Entry 身份，后续读写按
该 Entry 的最新规则状态重新授权；但这会影响硬链接的 inode 表现，需 Linux 验证。
这是适配层的独立语义决策，不在匹配优化里默认改变。

同理，unlink 后仍存活的句柄、覆盖后旧句柄、rename 后句柄和无句柄 inode 请求，
都必须有明确授权位置。仅保存“打开时允许”的布尔值会改变当前 read/write 再授权
行为。删除 inode->全路径表的门槛包括这些决策，而不只是新匹配器跑通。
符号链接打开目前还检查 canonicalize 后的宿主目标；新适配层必须继续检查命名
路径与解析目标，两者可能使用不同 Entry/Tree。核心不自行 canonicalize。

## 可独立测试的库与迁移步骤

建议后续建立独立 `crates/leash-policy` 包，不依赖 fuser、Landlock 或 `/proc`。
通过 `cargo test -q --manifest-path crates/leash-policy/Cargo.toml` 在 macOS 测试；
当前根包包含 Linux 代码，不能把根包整体可跨平台编译作为前提。

概念接口分成三个部分：

```text
compile(source, LoadResolvers) -> Program | ParseError
start(program, policy_root) -> PathState
advance(program, parent_state, name_bytes) -> PathState
evaluate(program, state, RequestContext, Facts) -> Decision | NeedFacts

Tree::intern_child(parent, name) -> EntryId
Tree::apply_committed(batch) -> ChangedEntries
Tree::refresh(entry, program, facts) -> ReadyState | NeedFacts
Tree::replace_program(program)
```

这些是职责示意，不是可直接复制编译的接口。LoadResolvers 注入 include、HOME、
exe/PATH、OS ID；FsCheck 和调用者探测放在 leash 适配层。Tree 管理 ID、版本、
拓扑与事实依赖，自动机只处理字节和规则 ID，便于分别测试。

迁移按以下门槛推进，每步独立提交：

1. 抽取现有解析/展开和评估语义，保留 globset 后端及注入式 FsCheck；迁移已有
   测试到独立包。暂不改 MirrorFs，也不在这个提交修正规则语义。
2. 实现残余状态后端和纯内存 Tree；差分结果全部一致后再作为可选执行器。
3. 接入 marker events、TTL 和 profile generation，先保持现有路径表作对照。
4. 在真实访问负载中测量后默认启用；再单独迁移 FUSE 到 EntryId。通过硬链接、
   句柄和 rename 测试后，删除双向完整路径表，宿主操作暂时按需构造路径。

测试必须覆盖：

- 旧 globset oracle 与新后端的内部条目集合、Visibility、错误码、匹配报告、
  条件探测顺序；每一级前缀推进后都比较，不只检查叶子。
- `*` 跨分隔符实际行为、`**` 零层/多层、`**/` 开头、根、尾斜杠、转义、字符类、
  含斜杠的分支、非法模式、非 UTF-8 文件名；不擅自规范化 `.`/重复分隔符输入。
- 顺序相反的可见后代与 deny、调用者差异、同规则多个内部条目、目标非目录。
- marker 创建/删除/覆盖/跨父 rename、嵌套 marker、多 marker AND、目录自身与
  孩子的差异、悬空链接、legacy 参数、外部事件漏报和过期。
- 随机内存树变更序列：每次变更后用完整路径 + 从头探测事实的简单模型比较；
  在同一个事实快照下比较，不把旧 TTL 的陈旧结果误认成语义标准。
- 多树别名、profile reload、节点回收复用、未发现节点和并发版本重试。
- Linux 后续验证：readdir 隐藏缓存、rename 两端授权、硬链接、打开后 unlink、
  目标覆盖与 symlink 目标授权。macOS 的纯库测试不能替代这些测试。

## 性能假设与验收

设 N 为缓存目录项数，L 为平均完整路径长度，B 为名称总字节数，A 为活跃 NFA
状态数，K 为当前接受的内部条目数，M 为相关 marker 数，S 为受影响缓存子树大小。

当前路径表持有多份路径，空间包含 O(NL)；新拓扑名称空间约 O(N+B)，但还需加上
节点索引、自动机共享状态和事实缓存。每节点复制 O(规则数) bitset 可能比路径还贵，
必须测实际驻留内存。新子节点推进约 O(名称长度 × A)，已有有效节点避免重复路径
匹配，授权仍需 O(K) 的最坏顺序检查以及条件探测。全局 `**` 较多时 A/K 可能很大；
不能保证 globset 已优化过的多模式匹配一定更慢。

分别测量编译耗时、冷树创建、热节点检查、深路径、多调用者、profile reload、
marker 失效和大目录 rename。记录 p50/p95/p99、分配次数、RSS、状态驻留命中率、
外部 probe 次数和失效后首次请求成本；对照相同规则与相同缓存时效的当前实现。
测试规则至少包含内置规则、大量互不相关绝对前缀、大量全局通配符和嵌套 marker。

启用门槛是语义差分无差异、缓存有界、外部变化处理明确，并在代表性负载中确有
收益。未达到性能门槛仍可保留独立纯库带来的测试价值，但不应为架构整齐强行替换。
