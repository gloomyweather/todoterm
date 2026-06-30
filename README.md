# todoterm
a minimalist and aesthetic terminal task manager written in Rust, featuring vim-like keybindings and a zero-noise monochrome design.

# **૮ ˶ᵔ ᵕ ᵔ˶ ა todo-tui**

a minimalist and aesthetic task manager for the terminal.

built with love in rust. fits perfectly into tiling window managers (like hyprland) and doesn't distract you from your work.

## **features**

* smooth console interface running at 60 fps  
* intuitive vim-like keybinds (keep your hands on the keyboard)  
* monochrome design with zero visual noise  
* ability to reorder tasks and set priorities  
* all data is safely and automatically saved to todo.json

## **how to install and run**

you will need the rust programming language installed (you can get it from the official rustup website).

1. clone the source code and navigate to the project folder:  
   git clone link  
   cd todo-tui

2. build and run the release (optimized) version:  
   cargo run \--release

*tip: you can copy the compiled binary from target/release/todo-tui to your \~/.local/bin/ folder to run the app from anywhere just by typing todo-tui in your terminal.*

## **keybinds**

all controls are designed so you never have to guess how to exit the program.

### **normal mode**

* j / k (or arrows) — navigate the list down/up  
* a (or i) — add a new task  
* d (or x) — permanently delete a task  
* enter (or space) — mark task as done / undone  
* m — enter task reordering mode  
* q — quit the app

### **insert mode**

* type text — the description of your new task  
* enter — save and return to normal mode  
* esc — cancel adding a task

### **move mode**

* j / k (or arrows) — move the selected task down/up  
* enter (or esc) — finish moving and save the order

may your tasks always be organized (=^･ω･^=)
