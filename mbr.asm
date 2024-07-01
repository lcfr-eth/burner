; stolen from Athenian
BITS 16
ORG     0x7c00

jmp start

start:
        mov ax,cs
        mov ds,ax
        mov si,msg
        call print

print:
        push ax
        cld
next:
        mov al,[si]
        cmp al,0
        je done
        call printchar
        inc si
        jmp next
done:
        jmp $

printchar:
        mov ah,0x0e
        int 0x10
        ret

msg:            db        "Hai Tavis...", 0

times 510 - ($-$$) db 0
dw        0xaa55
