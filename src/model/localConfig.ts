// 可写入客户端 JSON 配置文件的基础 JSON 值类型。
export type LocalConfigJsonValueModel =
    | string
    | number
    | boolean
    | null
    | LocalConfigJsonValueModel[]
    | { [key: string]: LocalConfigJsonValueModel };

// 客户端 JSON 配置文件的全量快照，用于前端启动和文件变化事件同步。
export type LocalConfigSnapshotModel = {
    // 配置文件版本号，用于后续结构升级和迁移判断。
    version: number;
    // 最近一次写入时间 ISO 字符串，外部手动修改时可能为空字符串。
    updatedAt: string;
    // 各模块按 StorageKey 分区保存的配置数据。
    items: Record<string, LocalConfigJsonValueModel>;
};

// 客户端 JSON 配置文件变化事件，用于文件变更后刷新所有已打开窗口。
export type LocalConfigChangedPayloadModel = LocalConfigSnapshotModel;
