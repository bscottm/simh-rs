;; -*- lexical-binding: t; -*-

;; Initial size of the GUI window
(setq initial-frame-alist '((top . 0) (width . 172) (height . 47)))
(setq default-frame-alist initial-frame-alist)

;; Emacs behavior customization
(put 'erase-buffer 'disabled nil)
(put 'narrow-to-region 'disabled nil)
(put 'scroll-left 'disabled nil)

(setq-default indent-tabs-mode nil)
(setq transient-mark-mode t)
(setq mouse-wheel-scroll-amount (quote (1 ((shift) . 1) ((control)))))

;; ;; Aerospace's proxy stuff
(defun aero-proxy-config ()
  (interactive)
  (setq url-proxy-services
    '(("no_proxy" . "^\\(localhost\\|10.*\\)")
      ("http" . "proxy-west.aero.org:8080")
      ("https" . "proxy-west.aero.org:8080")))
  (setq url-http-proxy-basic-auth-storage
    (list (list "proxy-west.aero.org:8080"
                (cons "Input your LDAP UID!"
                      (base64-encode-string (concat "21317:" (format-time-string "%a"))))))))

(defun aero-unproxy-config ()
  (interactive)
  (makunbound 'url-proxy-services)
  (makunbound 'url-http-proxy-basic-auth-storage))

;; Where specific programs are located:
;; Where specific programs are located:
(when (eq system-type 'windows-nt)
  (let* ((homedir (w32-short-file-name (or (getenv "USERPROFILE")
                                           (getenv "HOME"))))
         (scoop-dir (file-name-concat homedir "scoop" "apps" "msys2" "current"))
         (ucrt64-dir (file-name-concat scoop-dir "ucrt64" "bin"))
         (msys2-dir (file-name-concat scoop-dir "usr" "bin"))
         (places (seq-filter #'file-exists-p (list ucrt64-dir msys2-dir))))
    (setq exec-path (append places exec-path))
    (setq find-program (or (executable-find "find") find-program))
    (setq grep-program (or (executable-find "grep") grep-program))
    (if (file-exists-p msys2-dir)
        (setenv "PATH" (concat (subst-char-in-string ?/ ?\\ msys2-dir) path-separator (getenv "PATH"))))
    (if (file-exists-p ucrt64-dir)
        (setenv "PATH" (concat (subst-char-in-string ?/ ?\\ ucrt64-dir) path-separator (getenv "PATH"))))))

;; Don't make a new frame, just split the window.
(defvar display-buffer-same-window-commands
  '(compile-goto-error))

(defvar same-window-regexps (append same-window-regexps "\\*compilation\\*"))

(add-to-list 'display-buffer-alist
             '((lambda (&rest _args)
                 (memq this-command display-buffer-same-window-commands))
               (display-buffer-reuse-window
                display-buffer-same-window)
               (inhibit-same-window . nil)))

;; Loads the pkg-init.el[c] package initialization...
(add-to-list 'load-path (expand-file-name "lisp" user-emacs-directory))
(require 'pkg-init)

(custom-set-variables
 ;; custom-set-variables was added by Custom.
 ;; If you edit it by hand, you could mess it up, so be careful.
 ;; Your init file should contain only one such instance.
 ;; If there is more than one, they won't work right.
 '(column-number-mode t)
 '(tool-bar-mode nil))
(custom-set-faces
 ;; custom-set-faces was added by Custom.
 ;; If you edit it by hand, you could mess it up, so be careful.
 ;; Your init file should contain only one such instance.
 ;; If there is more than one, they won't work right.
 '(default ((t (:family "FantasqueSansM Nerd Font" :foundry "outline" :slant normal :weight regular :height 113 :width normal))))
 '(flymake-error ((t (:underline (:color "red" :style wave) :background unspecified :foreground "red"))))
 '(flymake-note ((t (:underline (:color "green" :style wave) :background unspecified :foreground "green"))))
 '(flymake-warning ((t (:underline (:color "orange" :style wave) :background unspecified :foreground "orange"))))
 '(whitespace-line ((t (:background "firebrick" :foreground "white" :weight bold)))))
