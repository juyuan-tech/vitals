# 模块参考

vitals 由 61 个模块组成，`vitals --list-modules` 可以列出全部模块。每个模块独立采集一项信息，互不依赖；`--sources` 可以查看每个模块在本次运行时**实际读了哪些文件**（运行时记录，不是手写的来源表）。本文按主题分组说明每个模块显示什么、在什么条件下有数据，以及它的依据文件。各条中「实际读取的文件」均逐字照抄 `--sources` 的运行时记录；写「没有读文件」的模块，其数据来自环境变量或系统调用。
> 说明：各条列出的路径都是 `--sources` 在作者机器上的运行时记录。含 `$HOME/` 的按你自己的家目录展开；某些模块只在特定桌面环境下才读某些文件（例如 GTK 的 `settings.ini`）。

## 系统身份与主机信息

- **os** — 发行版名称。
  实际读取的文件：`/etc/os-release`
- **host** — 机器型号，来自 DMI。
  实际读取的文件：`/sys/devices/virtual/dmi/id/sys_vendor`, `/sys/devices/virtual/dmi/id/product_name`, `/sys/devices/virtual/dmi/id/product_version`, `/sys/devices/virtual/dmi/id/board_name`
- **kernel** — 内核版本；走 `uname(2)`（经 rustix 的安全封装）而不是读 `/proc/sys/kernel/osrelease`，因此不绑死在 Linux 上。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **bios** — 固件信息，来自 DMI；键里会带 `UEFI` / `BIOS`，判断依据是 `/sys/firmware/efi` 在不在——同一条固件信息在两种引导方式下含义完全不同。
  实际读取的文件：`/sys/devices/virtual/dmi/id/bios_version`, `/sys/devices/virtual/dmi/id/bios_vendor`, `/sys/devices/virtual/dmi/id/bios_date`
- **board** — 主板型号，来自 DMI。
  实际读取的文件：`/sys/devices/virtual/dmi/id/board_name`, `/sys/devices/virtual/dmi/id/board_vendor`, `/sys/devices/virtual/dmi/id/board_version`
- **chassis** — 机箱类型（笔记本 / 台式机 / 服务器……），来自 DMI；DMI 的机箱类型比从 `/sys/class/power_supply/*` 之类间接推断更可靠。
  实际读取的文件：`/sys/devices/virtual/dmi/id/chassis_type`
- **version** — 本程序自己的版本与编译目标；数值全部来自编译期常量，运行时不变，平台用 `std::env::consts` 而不是 `uname`。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **rust** — 当前 rustup 工具链；只读文件、不开子进程，报的是工具链名（`stable-x86_64-unknown-linux-gnu`）而不是编译器版本号。
  实际读取的文件：`$HOME/.rustup/settings.toml`
- **uptime** — 开机时长。
  实际读取的文件：`/proc/uptime`
- **loadavg** — 1 / 5 / 15 分钟的平均负载；只做 Linux，非 Linux 上安静地不出数据，值原样保留内核给的两位小数、不做四舍五入。
  实际读取的文件：`/proc/loadavg`
- **processes** — 进程数与线程数；两个数都不需要打开任何进程的文件，进程数数 `/proc` 下的纯数字目录、线程数取 `/proc/loadavg` 第 4 个字段。
  实际读取的文件：`/proc/loadavg`
- **init-system** — 1 号进程（init）的名字与版本；版本没有免 fork 的通用来源，于是去包数据库里找，找不到版本就只显示名字（在容器里显示的是容器的 1 号进程）。
  实际读取的文件：`/proc/1/comm`

## CPU 与内存

- **cpu** — 只做型号与核心数，**不做占用率**：采集类工具给的是「身份与容量」的快照，占用率那种每时每刻都在变的东西属于 htop/btop。
  实际读取的文件：`/proc/cpuinfo`, `/sys/devices/system/cpu/present`
- **cpu-cache** — CPU 各级缓存的大小与共享核数；报的是「几个实例 × 每个多大」而不是总容量，L3 常被所有核共享、算出来是 1 时不写 `1x`。
  实际读取的文件：`/sys/devices/system/cpu/cpu0/cache/index0/level`, `/sys/devices/system/cpu/cpu0/cache/index0/type`, `/sys/devices/system/cpu/cpu0/cache/index0/size`, `/sys/devices/system/cpu/cpu0/cache/index0/shared_cpu_list`, `/sys/devices/system/cpu/cpu0/cache/index1/level`, `/sys/devices/system/cpu/cpu0/cache/index1/type`, `/sys/devices/system/cpu/cpu0/cache/index1/size`, `/sys/devices/system/cpu/cpu0/cache/index1/shared_cpu_list`, `/sys/devices/system/cpu/cpu0/cache/index2/level`, `/sys/devices/system/cpu/cpu0/cache/index2/type`, `/sys/devices/system/cpu/cpu0/cache/index2/size`, `/sys/devices/system/cpu/cpu0/cache/index2/shared_cpu_list`, `/sys/devices/system/cpu/cpu0/cache/index3/level`, `/sys/devices/system/cpu/cpu0/cache/index3/type`, `/sys/devices/system/cpu/cpu0/cache/index3/size`, `/sys/devices/system/cpu/cpu0/cache/index3/shared_cpu_list`, `/sys/devices/system/cpu/online`
- **memory** — 内存用量，与 Swap 同读 `/proc/meminfo`。
  实际读取的文件：`/proc/meminfo`
- **swap** — 交换空间用量；没开 swap 的机器（`SwapTotal = 0`）返回**无数据**，而不是 `0 B / 0 B (0%)`。
  实际读取的文件：`/proc/meminfo`
- **cpu-usage** — 整体 CPU 占用率；`/proc/stat` 是自开机以来累计的 jiffies，只能两次采样作差，跳过合计行、逐核取前 7 个数后再取算术平均。
  实际读取的文件：`/proc/stat`
- **top** — CPU 占用最高的几个进程（默认前 5）；跳过内核线程，CPU% 是「采样窗口内占满一个核」的百分比（可超过 100%），单个 pid 读不到就跳过、`io` 读不到读写都算 0、不让整个模块失败。
  实际读取的文件：`/proc/1/stat`, `/proc/1/status`, `/proc/1/io`, `/proc/2/stat`, `/proc/3/stat`, `/proc/4/stat`, `/proc/5/stat`, `/proc/6/stat`, `/proc/7/stat`, `/proc/8/stat`, `/proc/11/stat`, `/proc/14/stat`, `/proc/15/stat`, `/proc/16/stat`, `/proc/17/stat`, `/proc/18/stat`, `/proc/19/stat`, `/proc/20/stat`, `/proc/21/stat`, `/proc/22/stat`, `/proc/23/stat`, `/proc/24/stat`, `/proc/25/stat`, `/proc/26/stat`, `/proc/27/stat`, `/proc/29/stat`, `/proc/30/stat`, `/proc/31/stat`, `/proc/32/stat`, `/proc/33/stat`, `/proc/35/stat`, `/proc/36/stat`, `/proc/37/stat`, `/proc/38/stat`, `/proc/39/stat`, `/proc/41/stat`, `/proc/42/stat`, `/proc/43/stat`, `/proc/44/stat`, `/proc/45/stat`, `/proc/47/stat`, `/proc/48/stat`, `/proc/49/stat`, `/proc/50/stat`, `/proc/51/stat`, `/proc/53/stat`, `/proc/54/stat`, `/proc/55/stat`, `/proc/56/stat`, `/proc/57/stat`, `/proc/59/stat`, `/proc/60/stat`, `/proc/61/stat`, `/proc/62/stat`, `/proc/63/stat`, `/proc/65/stat`, `/proc/66/stat`, `/proc/67/stat`, `/proc/68/stat`, `/proc/69/stat`, `/proc/71/stat`, `/proc/72/stat`, `/proc/73/stat`, `/proc/74/stat`, `/proc/75/stat`, `/proc/77/stat`, `/proc/78/stat`, `/proc/79/stat`, `/proc/80/stat`, `/proc/81/stat`, `/proc/83/stat`, `/proc/84/stat`, `/proc/85/stat`, `/proc/86/stat`, `/proc/87/stat`, `/proc/89/stat`, `/proc/90/stat`, `/proc/91/stat`, `/proc/92/stat`, `/proc/93/stat`, `/proc/95/stat`, `/proc/96/stat`, `/proc/97/stat`, `/proc/98/stat`, `/proc/99/stat`, `/proc/101/stat`, `/proc/102/stat`, `/proc/103/stat`, `/proc/104/stat`, `/proc/105/stat`, `/proc/107/stat`, `/proc/108/stat`, `/proc/109/stat`, `/proc/110/stat`, `/proc/111/stat`, `/proc/113/stat`, `/proc/114/stat`, `/proc/115/stat`, `/proc/116/stat`, `/proc/117/stat`, `/proc/118/stat`, `/proc/119/stat`, `/proc/120/stat`, `/proc/122/stat`, `/proc/123/stat`, `/proc/124/stat`, `/proc/125/stat`, `/proc/126/stat`, `/proc/127/stat`, `/proc/128/stat`, `/proc/129/stat`, `/proc/131/stat`, `/proc/132/stat`, `/proc/133/stat`, `/proc/134/stat`, `/proc/135/stat`, `/proc/136/stat`, `/proc/139/stat`, `/proc/140/stat`, `/proc/141/stat`, `/proc/143/stat`, `/proc/150/stat`, `/proc/151/stat`, `/proc/152/stat`, `/proc/160/stat`, `/proc/161/stat`, `/proc/162/stat`, `/proc/163/stat`, `/proc/164/stat`, `/proc/165/stat`, `/proc/249/stat`, `/proc/250/stat`, `/proc/251/stat`, `/proc/252/stat`, `/proc/258/stat`, `/proc/266/stat`, `/proc/267/stat`, `/proc/268/stat`, `/proc/269/stat`, `/proc/270/stat`, `/proc/271/stat`, `/proc/272/stat`, `/proc/273/stat`, `/proc/274/stat`, `/proc/275/stat`, `/proc/276/stat`, `/proc/277/stat`, `/proc/278/stat`, `/proc/279/stat`, `/proc/286/stat`, `/proc/287/stat`, `/proc/288/stat`, `/proc/289/stat`, `/proc/290/stat`, `/proc/291/stat`, `/proc/292/stat`, `/proc/293/stat`, `/proc/294/stat`, `/proc/295/stat`, `/proc/296/stat`, `/proc/297/stat`, `/proc/298/stat`, `/proc/299/stat`, `/proc/300/stat`, `/proc/301/stat`, `/proc/302/stat`, `/proc/303/stat`, `/proc/304/stat`, `/proc/305/stat`, `/proc/306/stat`, `/proc/307/stat`, `/proc/308/stat`, `/proc/309/stat`, `/proc/310/stat`, `/proc/311/stat`, `/proc/312/stat`, `/proc/313/stat`, `/proc/314/stat`, `/proc/315/stat`, `/proc/326/stat`, `/proc/328/stat`, `/proc/329/stat`, `/proc/330/stat`, `/proc/331/stat`, `/proc/332/stat`, `/proc/337/stat`, `/proc/340/stat`, `/proc/342/stat`, `/proc/343/stat`, `/proc/344/stat`, `/proc/370/stat`, `/proc/372/stat`, `/proc/373/stat`, `/proc/375/stat`, `/proc/385/stat`, `/proc/385/status`, `/proc/385/io`, `/proc/394/stat`, `/proc/408/stat`, `/proc/408/status`, `/proc/408/io`, `/proc/415/stat`, `/proc/425/stat`, `/proc/425/status`, `/proc/425/io`, `/proc/428/stat`, `/proc/494/stat`, `/proc/512/stat`, `/proc/516/stat`, `/proc/526/stat`, `/proc/544/stat`, `/proc/546/stat`, `/proc/547/stat`, `/proc/548/stat`, `/proc/583/stat`, `/proc/588/stat`, `/proc/588/status`, `/proc/588/io`, `/proc/589/stat`, `/proc/589/status`, `/proc/589/io`, `/proc/590/stat`, `/proc/590/status`, `/proc/590/io`, `/proc/591/stat`, `/proc/591/status`, `/proc/591/io`, `/proc/593/stat`, `/proc/593/status`, `/proc/593/io`, `/proc/595/stat`, `/proc/595/status`, `/proc/595/io`, `/proc/685/stat`, `/proc/685/status`, `/proc/685/io`, `/proc/686/stat`, `/proc/686/status`, `/proc/686/io`, `/proc/722/stat`, `/proc/722/status`, `/proc/722/io`, `/proc/953/stat`, `/proc/953/status`, `/proc/953/io`, `/proc/979/stat`, `/proc/979/status`, `/proc/979/io`, `/proc/998/stat`, `/proc/998/status`, `/proc/998/io`, `/proc/1005/stat`, `/proc/1005/status`, `/proc/1005/io`, `/proc/1041/stat`, `/proc/1041/status`, `/proc/1041/io`, `/proc/1042/stat`, `/proc/1042/status`, `/proc/1042/io`, `/proc/1059/stat`, `/proc/1059/status`, `/proc/1059/io`, `/proc/1095/stat`, `/proc/1095/status`, `/proc/1095/io`, `/proc/1097/stat`, `/proc/1097/status`, `/proc/1097/io`, `/proc/1104/stat`, `/proc/1104/status`, `/proc/1104/io`, `/proc/1106/stat`, `/proc/1106/status`, `/proc/1106/io`, `/proc/1107/stat`, `/proc/1107/status`, `/proc/1107/io`, `/proc/1108/stat`, `/proc/1108/status`, `/proc/1108/io`, `/proc/1109/stat`, `/proc/1109/status`, `/proc/1109/io`, `/proc/1110/stat`, `/proc/1110/status`, `/proc/1110/io`, `/proc/1117/stat`, `/proc/1117/status`, `/proc/1117/io`, `/proc/1214/stat`, `/proc/1252/stat`, `/proc/1252/status`, `/proc/1252/io`, `/proc/1253/stat`, `/proc/1253/status`, `/proc/1253/io`, `/proc/1256/stat`, `/proc/1256/status`, `/proc/1256/io`, `/proc/1261/stat`, `/proc/1261/status`, `/proc/1261/io`, `/proc/1262/stat`, `/proc/1262/status`, `/proc/1262/io`, `/proc/1264/stat`, `/proc/1264/status`, `/proc/1264/io`, `/proc/1266/stat`, `/proc/1266/status`, `/proc/1266/io`, `/proc/1292/stat`, `/proc/1292/status`, `/proc/1292/io`, `/proc/1298/stat`, `/proc/1298/status`, `/proc/1298/io`, `/proc/1299/stat`, `/proc/1299/status`, `/proc/1299/io`, `/proc/1309/stat`, `/proc/1309/status`, `/proc/1309/io`, `/proc/1315/stat`, `/proc/1315/status`, `/proc/1315/io`, `/proc/1338/stat`, `/proc/1338/status`, `/proc/1338/io`, `/proc/1339/stat`, `/proc/1339/status`, `/proc/1339/io`, `/proc/1356/stat`, `/proc/1356/status`, `/proc/1356/io`, `/proc/1376/stat`, `/proc/1376/status`, `/proc/1376/io`, `/proc/1382/stat`, `/proc/1382/status`, `/proc/1382/io`, `/proc/1467/stat`, `/proc/1467/status`, `/proc/1467/io`, `/proc/1470/stat`, `/proc/1470/status`, `/proc/1470/io`, `/proc/1471/stat`, `/proc/1471/status`, `/proc/1471/io`, `/proc/1487/stat`, `/proc/1487/status`, `/proc/1487/io`, `/proc/1494/stat`, `/proc/1494/status`, `/proc/1494/io`, `/proc/1499/stat`, `/proc/1499/status`, `/proc/1499/io`, `/proc/1505/stat`, `/proc/1505/status`, `/proc/1505/io`, `/proc/1508/stat`, `/proc/1508/status`, `/proc/1508/io`, `/proc/1626/stat`, `/proc/1626/status`, `/proc/1626/io`, `/proc/1683/stat`, `/proc/1683/status`, `/proc/1683/io`, `/proc/1703/stat`, `/proc/1703/status`, `/proc/1703/io`, `/proc/1755/stat`, `/proc/1755/status`, `/proc/1755/io`, `/proc/2207/stat`, `/proc/2207/status`, `/proc/2207/io`, `/proc/2662/stat`, `/proc/2662/status`, `/proc/2662/io`, `/proc/2691/stat`, `/proc/2691/status`, `/proc/2692/stat`, `/proc/2692/status`, `/proc/2693/stat`, `/proc/2693/status`, `/proc/2694/stat`, `/proc/2694/status`, `/proc/2694/io`, `/proc/2708/stat`, `/proc/2708/status`, `/proc/2709/stat`, `/proc/2709/status`, `/proc/5851/stat`, `/proc/5851/status`, `/proc/5851/io`, `/proc/5855/stat`, `/proc/5855/status`, `/proc/5855/io`, `/proc/5865/stat`, `/proc/5865/status`, `/proc/5865/io`, `/proc/5878/stat`, `/proc/5878/status`, `/proc/5878/io`, `/proc/5879/stat`, `/proc/5879/status`, `/proc/5879/io`, `/proc/67741/stat`, `/proc/67741/status`, `/proc/67741/io`, `/proc/67755/stat`, `/proc/67755/status`, `/proc/67755/io`, `/proc/67757/stat`, `/proc/67757/status`, `/proc/67757/io`, `/proc/77943/stat`, `/proc/77943/status`, `/proc/77943/io`, `/proc/79637/stat`, `/proc/79638/stat`, `/proc/79639/stat`, `/proc/79643/stat`, `/proc/83503/stat`, `/proc/91722/stat`, `/proc/97175/stat`, `/proc/97175/status`, `/proc/97175/io`, `/proc/97186/stat`, `/proc/97186/status`, `/proc/97186/io`, `/proc/97193/stat`, `/proc/97193/status`, `/proc/97193/io`, `/proc/97198/stat`, `/proc/97198/status`, `/proc/97198/io`, `/proc/97716/stat`, `/proc/97716/status`, `/proc/97716/io`, `/proc/97840/stat`, `/proc/97840/status`, `/proc/97840/io`, `/proc/97845/stat`, `/proc/97845/status`, `/proc/97845/io`, `/proc/97942/stat`, `/proc/97942/status`, `/proc/97942/io`, `/proc/97960/stat`, `/proc/97960/status`, `/proc/97960/io`, `/proc/97991/stat`, `/proc/97991/status`, `/proc/97991/io`, `/proc/98107/stat`, `/proc/98107/status`, `/proc/98107/io`, `/proc/98150/stat`, `/proc/98150/status`, `/proc/98150/io`, `/proc/98317/stat`, `/proc/98317/status`, `/proc/98317/io`, `/proc/112239/stat`, `/proc/112239/status`, `/proc/112239/io`, `/proc/112242/stat`, `/proc/112242/status`, `/proc/112242/io`, `/proc/112246/stat`, `/proc/112246/status`, `/proc/112246/io`, `/proc/112247/stat`, `/proc/112247/status`, `/proc/112247/io`, `/proc/112280/stat`, `/proc/112280/status`, `/proc/112280/io`, `/proc/112286/stat`, `/proc/112286/status`, `/proc/112286/io`, `/proc/112342/stat`, `/proc/112342/status`, `/proc/112342/io`, `/proc/121722/stat`, `/proc/121722/status`, `/proc/121722/io`, `/proc/121741/stat`, `/proc/121741/status`, `/proc/121741/io`, `/proc/121748/stat`, `/proc/121748/status`, `/proc/121748/io`, `/proc/121752/stat`, `/proc/121752/status`, `/proc/121752/io`, `/proc/122145/stat`, `/proc/122145/status`, `/proc/122145/io`, `/proc/131720/stat`, `/proc/131720/status`, `/proc/131720/io`, `/proc/131739/stat`, `/proc/131739/status`, `/proc/131739/io`, `/proc/131746/stat`, `/proc/131746/status`, `/proc/131746/io`, `/proc/131752/stat`, `/proc/131752/status`, `/proc/131752/io`, `/proc/132147/stat`, `/proc/132147/status`, `/proc/132147/io`, `/proc/154555/stat`, `/proc/157552/stat`, `/proc/157552/status`, `/proc/157552/io`, `/proc/157650/stat`, `/proc/157650/status`, `/proc/157650/io`, `/proc/157725/stat`, `/proc/157725/status`, `/proc/157725/io`, `/proc/185895/stat`, `/proc/185895/status`, `/proc/185895/io`, `/proc/186637/stat`, `/proc/186637/status`, `/proc/186637/io`, `/proc/186643/stat`, `/proc/186643/status`, `/proc/186643/io`, `/proc/188681/stat`, `/proc/188681/status`, `/proc/188681/io`, `/proc/206172/stat`, `/proc/206172/status`, `/proc/206172/io`, `/proc/210090/stat`, `/proc/210090/status`, `/proc/210090/io`, `/proc/210103/stat`, `/proc/210103/status`, `/proc/210103/io`, `/proc/210110/stat`, `/proc/210110/status`, `/proc/210110/io`, `/proc/210115/stat`, `/proc/210115/status`, `/proc/210115/io`, `/proc/217613/stat`, `/proc/217613/status`, `/proc/217613/io`, `/proc/217616/stat`, `/proc/217616/status`, `/proc/217616/io`, `/proc/217617/stat`, `/proc/217617/status`, `/proc/217617/io`, `/proc/217630/stat`, `/proc/217630/status`, `/proc/217630/io`, `/proc/217646/stat`, `/proc/217646/status`, `/proc/217646/io`, `/proc/217664/stat`, `/proc/217664/status`, `/proc/217664/io`, `/proc/220568/stat`, `/proc/237538/stat`, `/proc/237538/status`, `/proc/237538/io`, `/proc/240468/stat`, `/proc/262913/stat`, `/proc/262916/stat`, `/proc/273560/stat`, `/proc/306508/stat`, `/proc/306508/status`, `/proc/306508/io`, `/proc/310855/stat`, `/proc/319688/stat`, `/proc/341218/stat`, `/proc/341218/status`, `/proc/341218/io`, `/proc/341319/stat`, `/proc/341319/status`, `/proc/341319/io`, `/proc/354769/stat`, `/proc/361659/stat`, `/proc/361659/status`, `/proc/361659/io`, `/proc/361662/stat`, `/proc/361662/status`, `/proc/361662/io`, `/proc/361663/stat`, `/proc/361663/status`, `/proc/361663/io`, `/proc/361665/stat`, `/proc/361665/status`, `/proc/361665/io`, `/proc/361683/stat`, `/proc/361683/status`, `/proc/361683/io`, `/proc/361710/stat`, `/proc/361710/status`, `/proc/361710/io`, `/proc/361713/stat`, `/proc/361713/status`, `/proc/361713/io`, `/proc/361759/stat`, `/proc/361759/status`, `/proc/361759/io`, `/proc/361782/stat`, `/proc/361782/status`, `/proc/361782/io`, `/proc/361804/stat`, `/proc/361804/status`, `/proc/361804/io`, `/proc/361871/stat`, `/proc/361871/status`, `/proc/361871/io`, `/proc/361962/stat`, `/proc/361962/status`, `/proc/361962/io`, `/proc/362115/stat`, `/proc/362115/status`, `/proc/362115/io`, `/proc/362135/stat`, `/proc/362135/status`, `/proc/362135/io`, `/proc/363467/stat`, `/proc/363467/status`, `/proc/363467/io`, `/proc/364498/stat`, `/proc/396363/stat`, `/proc/443951/stat`, `/proc/446349/stat`, `/proc/496945/stat`, `/proc/496946/stat`, `/proc/521649/stat`, `/proc/525924/stat`, `/proc/556893/stat`, `/proc/556894/stat`, `/proc/589596/stat`, `/proc/589596/status`, `/proc/589596/io`, `/proc/590756/stat`, `/proc/590978/stat`, `/proc/591598/stat`, `/proc/599234/stat`, `/proc/599235/stat`, `/proc/599237/stat`, `/proc/599238/stat`, `/proc/608745/stat`, `/proc/611367/stat`, `/proc/611370/stat`, `/proc/619075/stat`, `/proc/632656/stat`, `/proc/640181/stat`, `/proc/641257/stat`, `/proc/643213/stat`, `/proc/643213/status`, `/proc/643213/io`, `/proc/643400/stat`, `/proc/643400/status`, `/proc/643400/io`, `/proc/643536/stat`, `/proc/643537/stat`, `/proc/643539/stat`, `/proc/643874/stat`, `/proc/643983/stat`, `/proc/643983/status`, `/proc/643983/io`, `/proc/644111/stat`, `/proc/644206/stat`, `/proc/648251/stat`, `/proc/648808/stat`, `/proc/648888/stat`, `/proc/651176/stat`, `/proc/651177/stat`, `/proc/651475/stat`, `/proc/651495/stat`, `/proc/651593/stat`, `/proc/651811/stat`, `/proc/651970/stat`, `/proc/652264/stat`, `/proc/654397/stat`, `/proc/654397/status`, `/proc/654397/io`, `/proc/654449/stat`, `/proc/654684/stat`, `/proc/655986/stat`, `/proc/656117/stat`, `/proc/656142/stat`, `/proc/656275/stat`, `/proc/656563/stat`, `/proc/656564/stat`, `/proc/656565/stat`, `/proc/656566/stat`, `/proc/656567/stat`, `/proc/656568/stat`, `/proc/656916/stat`, `/proc/656916/status`, `/proc/656916/io`, `/proc/657330/stat`, `/proc/657398/stat`, `/proc/657414/stat`, `/proc/657547/stat`, `/proc/657547/status`, `/proc/657547/io`, `/proc/657556/stat`, `/proc/657618/stat`, `/proc/657618/status`, `/proc/657618/io`, `/proc/657668/stat`, `/proc/657668/status`, `/proc/657668/io`, `/proc/657750/stat`, `/proc/657845/stat`, `/proc/657845/status`, `/proc/657845/io`, `/proc/657889/stat`, `/proc/657941/stat`, `/proc/657941/status`, `/proc/657941/io`, `/proc/658325/stat`, `/proc/658445/stat`, `/proc/658753/stat`, `/proc/658753/status`, `/proc/658753/io`, `/proc/658787/stat`, `/proc/658787/status`, `/proc/658787/io`

## 磁盘与文件系统

- **disk** — 根文件系统的用量；v0.1 只看 `/`。
  实际读取的文件：`/proc/mounts`
- **physical-disk** — 物理盘；判据是有没有 `device` 符号链接，容量单位固定是 512 字节扇区，读不到厂商/型号就退回设备名，排除 `loop*` 与 `dm-*`。
  实际读取的文件：`/sys/block/nvme0n1/size`, `/sys/block/nvme0n1/removable`, `/sys/block/nvme0n1/device/vendor`, `/sys/block/nvme0n1/device/model`, `/sys/block/nvme0n1/queue/rotational`, `/sys/block/sda/size`, `/sys/block/sda/removable`, `/sys/block/sda/device/vendor`, `/sys/block/sda/device/model`, `/sys/block/sda/queue/rotational`, `/sys/block/zram0/size`, `/sys/block/zram0/removable`
- **disk-io** — 物理盘的读写速率；`/sys/block/<盘>/stat` 是开机至今的累计值，只能两次采样作差，且只报有 `device` 链接的盘（所以盘数可能与 physical-disk 不同，不是 bug）。
  实际读取的文件：`/sys/block/nvme0n1/stat`, `/sys/block/nvme0n1/device/vendor`, `/sys/block/nvme0n1/device/model`, `/sys/block/sda/stat`, `/sys/block/sda/device/vendor`, `/sys/block/sda/device/model`
- **btrfs** — 每个 btrfs 文件系统的容量与空间分配；「已用」取 `disk_used` 之和（不是 `bytes_used`），「已分配」不等于「已用」，设备容量之和 > 0 才算一个能报的文件系统。
  实际读取的文件：`/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/label`, `/sys/class/block/nvme0n1p3/size`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/data/disk_total`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/data/disk_used`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/metadata/disk_total`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/metadata/disk_used`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/system/disk_total`, `/sys/fs/btrfs/64289e72-90af-45ff-8ad4-58c2a0ddd702/allocation/system/disk_used`
- **bootmgr** — 二级启动器（UEFI 启动项，或磁盘上的启动器配置）；首选 UEFI 变量，拿不到 efivars 时回退磁盘线索，都没有就无数据。
  实际读取的文件：`/sys/firmware/efi/efivars/BootCurrent-8be4df61-93ca-11d2-aa0d-00e098032b8c`, `/sys/firmware/efi/efivars/Boot0002-8be4df61-93ca-11d2-aa0d-00e098032b8c`

## 电源与固件

- **battery** — 笔记本电池的电量与状态；只认 `type` 为 `Battery` 的设备，`status` 只有三个值会印出来（不含 `Full`），`scope` 为 `Device` 的外设电池与 `present` 为 `0` 的空电池仓都不算，台式机没有电池则无数据。
  实际读取的文件：`/sys/class/power_supply/ucsi-source-psy-USBC000:001/type`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/scope`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/present`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/capacity`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/model_name`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/status`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/online`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/energy_now`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/energy_full`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/charge_now`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/charge_full`, `/sys/class/power_supply/ADP1/type`, `/sys/class/power_supply/ADP1/scope`, `/sys/class/power_supply/ADP1/present`, `/sys/class/power_supply/ADP1/capacity`, `/sys/class/power_supply/ADP1/model_name`, `/sys/class/power_supply/ADP1/status`, `/sys/class/power_supply/ADP1/online`, `/sys/class/power_supply/ADP1/energy_now`, `/sys/class/power_supply/ADP1/energy_full`, `/sys/class/power_supply/ADP1/charge_now`, `/sys/class/power_supply/ADP1/charge_full`, `/sys/class/power_supply/BAT0/type`, `/sys/class/power_supply/BAT0/scope`, `/sys/class/power_supply/BAT0/present`, `/sys/class/power_supply/BAT0/capacity`, `/sys/class/power_supply/BAT0/model_name`, `/sys/class/power_supply/BAT0/status`, `/sys/class/power_supply/BAT0/online`, `/sys/class/power_supply/BAT0/energy_now`, `/sys/class/power_supply/BAT0/energy_full`, `/sys/class/power_supply/BAT0/charge_now`, `/sys/class/power_supply/BAT0/charge_full`
- **power-adapter** — 外接电源接上了没有；只认 `type` 为 `Mains` 的设备并聚合成一行，`online` 读不到的设备不投票，所有 Mains 都读不到时整个模块报无数据；与 Battery 读同一目录。
  实际读取的文件：`/sys/class/power_supply/ucsi-source-psy-USBC000:001/type`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/scope`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/present`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/capacity`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/model_name`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/status`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/online`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/energy_now`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/energy_full`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/charge_now`, `/sys/class/power_supply/ucsi-source-psy-USBC000:001/charge_full`, `/sys/class/power_supply/ADP1/type`, `/sys/class/power_supply/ADP1/scope`, `/sys/class/power_supply/ADP1/present`, `/sys/class/power_supply/ADP1/capacity`, `/sys/class/power_supply/ADP1/model_name`, `/sys/class/power_supply/ADP1/status`, `/sys/class/power_supply/ADP1/online`, `/sys/class/power_supply/ADP1/energy_now`, `/sys/class/power_supply/ADP1/energy_full`, `/sys/class/power_supply/ADP1/charge_now`, `/sys/class/power_supply/ADP1/charge_full`, `/sys/class/power_supply/BAT0/type`, `/sys/class/power_supply/BAT0/scope`, `/sys/class/power_supply/BAT0/present`, `/sys/class/power_supply/BAT0/capacity`, `/sys/class/power_supply/BAT0/model_name`, `/sys/class/power_supply/BAT0/status`, `/sys/class/power_supply/BAT0/online`, `/sys/class/power_supply/BAT0/energy_now`, `/sys/class/power_supply/BAT0/energy_full`, `/sys/class/power_supply/BAT0/charge_now`, `/sys/class/power_supply/BAT0/charge_full`
- **brightness** — 背光亮度；`brightness` 与 `max_brightness` 缺一个就跳过该设备，可以有多个设备、一行一个，台式机没有背光则无数据。
  实际读取的文件：`/sys/class/backlight/amdgpu_bl1/brightness`, `/sys/class/backlight/amdgpu_bl1/max_brightness`
- **tpm** — 可信平台模块的版本与厂商；设备名不一定是 `tpm0` 所以遍历目录取第一个，版本只把 `"2"` 写成 `"2.0"`（别的值原样显示），厂商属性不在内核 ABI 文档里、部分内核没有该文件、那时只显示版本，完全没有 TPM 则无数据。
  实际读取的文件：`/sys/class/tpm/tpm0/tpm_version_major`, `/sys/class/tpm/tpm0/device/description`

## 网络

- **local-ip** — 本机地址，**默认路由用的那一个**；靠「UDP connect 探测路由」问出，不发任何字节，没有默认路由（断网、只有回环、纯离线）就是无数据，以 IPv4 为主、只有拿不到 IPv4 时才退到 IPv6。
  实际读取的文件：`/proc/net/route`, `/sys/class/net/enp5s0f4u1u3c2/address`
- **net-io** — 网卡的收发速率；字节计数器是开机至今的累计值，只能两次采样作差（采样间隔 200 ms，顺序调度下整体为此慢 200 ms），且只报默认路由那一张网卡。
  实际读取的文件：`/proc/net/route`, `/sys/class/net/enp5s0f4u1u3c2/operstate`, `/sys/class/net/enp5s0f4u1u3c2/statistics/rx_bytes`, `/sys/class/net/enp5s0f4u1u3c2/statistics/tx_bytes`
- **wifi** — 无线网卡的状态与信号；判「哪些接口是无线」看对应目录在不在，信号质量与电平只在连上时才有数据行，报不出 SSID，一块无线网卡都没有则无数据。
  实际读取的文件：`/proc/net/wireless`, `/sys/class/net/wlan0/operstate`
- **dns** — `/etc/resolv.conf` 里配置的域名服务器；按文件顺序原样列出、不排序也不去重，关键字大小写敏感，只在全是回环 stub 时才改读 systemd-resolved 维护的那份文件，没有 `nameserver` 行或文件不存在则无数据。
  实际读取的文件：`/etc/resolv.conf`
- **de** — 桌面环境；线索与版本号来源与会话识别共用，没有桌面（纯 WM 会话、TTY、容器）就是无数据。
  实际读取的文件：`/proc/658787/stat`, `/proc/658753/stat`, `/proc/217630/stat`, `/proc/217616/stat`, `/proc/217613/stat`, `/proc/97193/stat`, `/proc/97175/stat`, `/proc/1095/stat`
- **wm** — 窗口管理器 / 合成器；值后面会带上会话类型（同一个 Mutter 在 Wayland 与 X11 上是两套东西），会话类型来自 `XDG_SESSION_TYPE`，读不到就不写括号。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）

## 会话、用户与桌面外观

- **user** — 当前用户名；「我是谁」以 uid 为准，环境变量只兜底。
  实际读取的文件：`/proc/self/status`, `/etc/passwd`
- **users** — 当前登录的用户会话；数据源是那个二进制文件而不是 `who`（零子进程），读到坏记录就跳过这一条，只统计 `USER_PROCESS`，按用户名去重并报最近那次登录，查不到时区就不印时间只报用户名；Debian/Ubuntu 已不再写该文件、在那类系统上就是无数据。
  实际读取的文件：`/var/run/utmp`, `/etc/localtime`
- **title** — `用户@主机名`，默认视图的第一行；用户名以 uid 为准、主机名走 `uname(2)` 的 nodename，注意这不是 Host 模块那个「机器型号」、笔记本上两者完全不同。
  实际读取的文件：`/proc/self/status`, `/etc/passwd`
- **shell** — 用户当前在用的 shell；只报名字，不报版本（拿版本得把 shell 本身跑起来，本版不开子进程）。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **terminal** — 当前终端；三个来源按可靠性排序（环境变量指纹 → 父进程链 → `$TERM` 兜底），全程不 fork。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **locale** — 当前区域设置；顺序照 POSIX（`LC_ALL` > `LC_CTYPE` > `LANG`，前面非空才轮到后面），环境变量一个都没有时退到全局配置，仍然没有就算无数据。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **editor** — 默认编辑器；顺序照 POSIX，`$VISUAL` 给全屏编辑器用、`$EDITOR` 是通用的（前者更具体），值是**名字**不是路径。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **lm** — 登录管理器（display manager）；四处来源都按可信度排，四处都没有就是**没装**显示管理器、那时报 `login`（这不是「查不到」，是查到了「没有」）；版本查包数据库，且**只能报「配的是哪个」、不能报「现在跑的是哪个」**。
  实际读取的文件：`/etc/X11/default-display-manager`, `/etc/conf.d/xdm`, `/etc/sysconfig/displaymanager`
- **theme** — 界面主题名（控件那一套）；GTK 系与 KDE 系两边都读，用户写下的配置排在发行版默认前面、同一级之内 GTK 先于 KDE，值后面标出**确实读到**的那一边，读不到就是无数据（宁可不显示，也不拿 `de` / `wm` 的名字倒推）。
  实际读取的文件：`$HOME/.config/gtk-3.0/settings.ini`, `$HOME/.config/gtk-4.0/settings.ini`, `$HOME/.config/kdeglobals`, `/etc/xdg/gtk-3.0/settings.ini`, `/etc/xdg/gtk-4.0/settings.ini`, `/etc/gtk-3.0/settings.ini`, `/etc/gtk-4.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-3.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-4.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-3.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-4.0/settings.ini`, `/usr/local/share/gtk-3.0/settings.ini`, `/usr/local/share/gtk-4.0/settings.ini`, `/usr/share/gtk-3.0/settings.ini`
- **icons** — 图标主题名；与 theme 一样的两支来源（键不同），特意**不**拿光标主题的兜底声明顶替，读不到就是无数据。
  实际读取的文件：`$HOME/.config/gtk-3.0/settings.ini`, `$HOME/.config/gtk-4.0/settings.ini`, `$HOME/.config/kdeglobals`, `/etc/xdg/gtk-3.0/settings.ini`, `/etc/xdg/gtk-4.0/settings.ini`, `/etc/gtk-3.0/settings.ini`, `/etc/gtk-4.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-3.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-4.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-3.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-4.0/settings.ini`, `/usr/local/share/gtk-3.0/settings.ini`, `/usr/local/share/gtk-4.0/settings.ini`, `/usr/share/gtk-3.0/settings.ini`
- **font** — 界面字体；GTK 的名称原样用、KDE 的机器串必须解析，两边输出统一成「名字 + 字号」，字号拿不到就只给名字（宁可少给一个数，也不编一个 0），读不到就是无数据。
  实际读取的文件：`$HOME/.config/gtk-3.0/settings.ini`, `$HOME/.config/gtk-4.0/settings.ini`, `$HOME/.config/kdeglobals`, `/etc/xdg/gtk-3.0/settings.ini`, `/etc/xdg/gtk-4.0/settings.ini`, `/etc/gtk-3.0/settings.ini`, `/etc/gtk-4.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-3.0/settings.ini`, `$HOME/.local/share/flatpak/exports/share/gtk-4.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-3.0/settings.ini`, `/var/lib/flatpak/exports/share/gtk-4.0/settings.ini`, `/usr/local/share/gtk-3.0/settings.ini`, `/usr/local/share/gtk-4.0/settings.ini`, `/usr/share/gtk-3.0/settings.ini`
- **cursor** — 光标（鼠标指针）主题名；先看会话环境（`$XCURSOR_THEME` 配 `$XCURSOR_SIZE`），会话里没有才退回配置线索、顺序是「用户级在发行版默认之前」，两个来源都没有就是无数据。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **wmtheme** — 窗口装饰主题（标题栏、边框那一套）；**只有 KDE 读得到**，只写了装饰库的名字而没有 `theme` 时是无数据（库名不是主题名），别的桌面宁可不显示这一行、也不拿 `wm` 的名字去凑一个值。
  实际读取的文件：`$HOME/.config/kwinrc`, `/etc/xdg/kwinrc`
- **sound** — 正在用的声音服务（PipeWire / PulseAudio / ALSA）；零子进程拿不到「正在出声的设备 + 音量」，所以报的是服务本身外加版本（版本查包数据库），声卡名字放进变量给 JSON 用；认服务只看会话目录里有没有它的 socket，顺序要紧（`pipewire-pulse` 同时提供 PulseAudio 的 socket），会话里没有声音服务时退回 `ALSA`，既没有服务也没有声卡则无数据。
  实际读取的文件：`/proc/asound/cards`

## 外设与显示

- **display** — 接了哪些显示器、各自多大、多少赫兹；`status` 为 `connected` 才算接了显示器，`modes` 第一行是首选模式（sysfs 里没有「当前模式」字段），刷新率只在算出来的分辨率与首选模式一致时才敢报。
  实际读取的文件：`/sys/class/drm/card1-DP-1/status`, `/sys/class/drm/card1-DP-2/status`, `/sys/class/drm/card1-DP-3/status`, `/sys/class/drm/card1-DP-4/status`, `/sys/class/drm/card1-DP-5/status`, `/sys/class/drm/card1-DP-6/status`, `/sys/class/drm/card1-HDMI-A-1/status`, `/sys/class/drm/card1-Writeback-1/status`, `/sys/class/drm/card1-eDP-1/status`, `/sys/class/drm/card1-eDP-1/modes`, `/sys/class/drm/card1-eDP-1/edid`
- **monitor** — 显示器的物理参数；与 `display` 是同一份数据的两种摆法（这里印参数本身，连三位小数的刷新率都要），两处换算见注释，数据不全就不印（没有数据不是错误）。
  实际读取的文件：`/sys/class/drm/card1-DP-1/status`, `/sys/class/drm/card1-DP-2/status`, `/sys/class/drm/card1-DP-3/status`, `/sys/class/drm/card1-DP-4/status`, `/sys/class/drm/card1-DP-5/status`, `/sys/class/drm/card1-DP-6/status`, `/sys/class/drm/card1-HDMI-A-1/status`, `/sys/class/drm/card1-Writeback-1/status`, `/sys/class/drm/card1-eDP-1/status`, `/sys/class/drm/card1-eDP-1/modes`, `/sys/class/drm/card1-eDP-1/edid`
- **gpu** — 显卡型号、驱动、独显还是核显；数据全在 sysfs 里，型号名从 `pci.ids` 查、查不到就报 PCI ID 本身（编一个名字才是撒谎），**不跑 `lspci`、不跑 `glxinfo`**，没有 PCI 显卡的机器就是无数据。
  实际读取的文件：`/sys/class/drm/card1/device/vendor`, `/sys/class/drm/card1/device/device`, `/sys/class/drm/card1/device/uevent`, `/sys/class/drm/card1/device/boot_vga`, `/usr/share/hwdata/pci.ids`, `/proc/cpuinfo`
- **camera** — 摄像头（V4L2 设备）；**必须按名字去重**（同一块摄像头会开出多个节点），报不出分辨率与像素格式（那要 V4L2 的 ioctl），所以值就是内核给的名字本身，没有摄像头则无数据。
  实际读取的文件：`/sys/class/video4linux/video0/name`, `/sys/class/video4linux/video1/name`, `/sys/class/video4linux/video2/name`, `/sys/class/video4linux/video3/name`
- **keyboard** — 键盘设备；靠按键位图而不是 `Handlers` 里的 `kbd` 判键盘（电源键也带 `kbd`），值就是设备名，多块键盘才编号、只有一块时不写序号。
  实际读取的文件：`/proc/bus/input/devices`
- **mouse** — 指针设备（鼠标、触摸板、轨迹球、触摸屏）；判据是指针那一带的按键而不是名字（无线接收器里那份键盘带 `BTN_0` 却不带 `BTN_LEFT`，只报在 Keyboard 那边），多块指针设备才编号。
  实际读取的文件：`/proc/bus/input/devices`
- **gamepad** — 游戏手柄；判据是 `H: Handlers=` 里有没有 `js*`（`eventN` 不算——鼠标键盘也都开着它），值就是设备名、多个手柄才编号；本机没有手柄因此无数据。
  实际读取的文件：`/proc/bus/input/devices`
- **terminal-font** — 终端自己用的那套字体；从终端自己的配置文件里读（没有环境变量或 `/proc` 能问出「我在用哪套字体」），**不跑 `kitty +kitten`、不跑 `fc-match`**，认不出终端或配置里没写字体就是无数据（**不猜**：报错了比不报更糟）。
  实际读取的文件：`$HOME/.config/kitty/kitty.conf`
- **terminal-size** — 终端尺寸（列 × 行）；与文本渲染器量宽度用的是同一个调用，拿不到（输出被重定向、或者根本不在终端里）就返回无数据。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **datetime** — 当前本地时间；标准库只给 UTC，本地时间要自己读时区库算，时区来源按顺序取 `$TZ` 或 `/etc/localtime`，真读不到就退回 UTC 并在变量里说明、绝不瞎猜一个偏移。
  实际读取的文件：`/etc/localtime`

## 软件包与管理器

- **packages** — 已安装包的数量，按包管理器分别列出、按名字字母序排（不按发现顺序），数量为 0 的包管理器不出现在结果里、一个都数不到则无数据；flatpak 只数应用、不数 runtime（这是刻意偏差）。
  实际读取的文件：`/var/lib/dpkg/status`

## 排版原语

- **separator** — 一条横线；它是**渲染原语**，不是采集到的数据（线该多长取决于别的信息行有多宽），所以这里只发一个空条目当标记：文本渲染器把它铺成横线，JSON 渲染器直接跳过它。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **break** — 一个空行；与 separator 一样是渲染原语，但连特判都不需要，存在的意义是把信息分组。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）
- **colors** — 一排 16 个色块；**采集器什么也不采**，这一块完全由渲染器画（发一个空条目当标记，渲染器见到这个模块就铺两行色块），JSON 输出里不会出现。
  实际读取的文件：没有读文件（数据来自环境变量或系统调用）

## 提示

- 用 `--module` 点名：`--module` 取一个**列表**，逗号分隔，只显示这些模块；顺序就是你写的顺序，配置里没有的也能点。
- `--sources` 与 `--explain` 的分工：`--explain` 说**状态**（显示 / 空 / 跳过 / 失败），`--sources` 说**依据**（读了哪个文件）。两个都给就先打状态、再打依据。要分辨「空」与「显示」，`--explain` 得真跑一遍采集，所以会花一次采集的时间。
- `--sources` 的内容是**运行时记录**，不是手写的来源表；同一模块在不同机器上读到的路径可能不同（例如 DMI、`/sys/class/drm/` 下的连接器、`/sys/class/power_supply/` 下的设备都会随机器而变）。
- 想看到结构化结果用 `--json`；`--json` 会自动关掉颜色与 Logo。
