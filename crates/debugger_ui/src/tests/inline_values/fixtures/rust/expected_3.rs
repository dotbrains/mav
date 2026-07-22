    static mut GLOBAL: usize = 1;

    fn main() {
        let x: 10 = 10;
        let value: 42 = 42;
        let y: 4 = 4;
        let tester = {
            let y = 10;
            let y = 5;
            let b = 3;
            vec![y, 20, 30]
        };

        let caller = || {
            let x = 3;
            println!("x={}", x);
        };

        caller();

        unsafe {
            GLOBAL = 2;
        }

        let result = value * 2 * x;
        println!("Simple test executed: value={}, result={}", value, result);
        assert!(true);
    }
