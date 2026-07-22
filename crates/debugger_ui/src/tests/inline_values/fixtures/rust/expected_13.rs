    static mut GLOBAL: usize = 1;

    fn main() {
        let x: 10 = 10;
        let value: 42 = 42;
        let y: 4 = 4;
        let tester: size=3 = {
            let y = 10;
            let y = 5;
            let b = 3;
            vec![y, 20, 30]
        };

        let caller: <not available> = || {
            let x = 3;
            println!("x={}", x);
        };

        caller();

        unsafe {
            GLOBAL = 2;
        }

        let result = value: 42 * 2 * x: 10;
        println!("Simple test executed: value={}, result={}", value, result);
        assert!(true);
    }
