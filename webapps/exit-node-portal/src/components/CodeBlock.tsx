import { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from './ui/button';
import { cn } from '@/lib/utils';

interface CodeBlockProps {
  code: string;
  title?: string;
  className?: string;
}

export function CodeBlock({ code, title, className }: CodeBlockProps) {
  const [copied, setCopied] = useState(false);

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      toast.success('Copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error('Failed to copy');
    }
  };

  return (
    <div className={cn('rounded-lg overflow-hidden', className)}>
      {title && (
        <div className="bg-muted px-4 py-2 text-sm text-muted-foreground border-b border-border">
          {title}
        </div>
      )}
      <div className="bg-muted/50 p-4 flex items-center justify-between gap-4">
        <code className="text-primary font-mono text-sm break-all">{code}</code>
        <Button
          onClick={copyToClipboard}
          variant="secondary"
          size="sm"
          className="flex-shrink-0 gap-2"
        >
          {copied ? (
            <>
              <Check className="h-4 w-4" />
              Copied
            </>
          ) : (
            <>
              <Copy className="h-4 w-4" />
              Copy
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
