import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { FolderIcon, FolderPlusIcon } from "lucide-react";
import { useEffect, useState } from "react";

function directoryName(directory: string) {
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

export function CreateProjectDialog({
  open,
  onOpenChange,
  onChooseDirectory,
  onCreate,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChooseDirectory: () => Promise<string | null>;
  onCreate: (name: string, directory: string) => void;
}) {
  const [name, setName] = useState("");
  const [directory, setDirectory] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setName("");
      setDirectory(null);
    }
  }, [open]);

  const chooseDirectory = async () => {
    const selected = await onChooseDirectory();
    if (!selected) return;
    setDirectory(selected);
    if (!name.trim()) setName(directoryName(selected));
  };

  const submit = () => {
    if (!directory || !name.trim()) return;
    onCreate(name.trim(), directory);
    onOpenChange(false);
  };

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent aria-describedby="create-project-description" className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>创建项目</DialogTitle>
          <DialogDescription id="create-project-description">
            添加一个 Pi 可读取和编辑的本地文件夹。
          </DialogDescription>
        </DialogHeader>

        <label className="grid gap-2 text-sm font-medium">
          项目名称
          <span className="flex h-12 items-center gap-3 rounded-xl border bg-background px-4 focus-within:ring-2 focus-within:ring-ring/50">
            <FolderIcon aria-hidden="true" className="size-4 text-muted-foreground" />
            <input
              aria-label="项目名称"
              className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-muted-foreground"
              onChange={(event) => setName(event.target.value)}
              placeholder="项目名称"
              value={name}
            />
          </span>
        </label>

        <div className="grid gap-2 text-sm font-medium">
          源文件夹
          <button
            className="flex min-h-28 w-full flex-col items-center justify-center gap-2 rounded-xl border bg-background px-5 py-4 text-center hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none"
            onClick={() => void chooseDirectory()}
            type="button"
          >
            <FolderPlusIcon aria-hidden="true" className="size-5 text-muted-foreground" />
            {directory ? (
              <>
                <span className="font-medium">{directoryName(directory)}</span>
                <span className="max-w-full truncate text-xs font-normal text-muted-foreground">
                  {directory}
                </span>
              </>
            ) : (
              <span>选择 Pi 可读取和编辑的文件夹</span>
            )}
          </button>
        </div>

        <DialogFooter>
          <Button onClick={() => onOpenChange(false)} type="button" variant="ghost">
            取消
          </Button>
          <Button disabled={!directory || !name.trim()} onClick={submit} type="button">
            创建项目
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
