extern crate gj;

#[test]
fn immediate() {
    ::gj::EventLoop::init();
    {
        let promise = ::gj::Promise::fulfilled(17u64);
        let value = promise.wait().unwrap();
        assert_eq!(value, 17);
    }

    {
        let promise = ::gj::Promise::fulfilled(19u64).map(|x| {
            assert_eq!(x, 19);
            return Ok(x + 2);
        });
        let value = promise.wait().unwrap();
        assert_eq!(value, 21);
    }
}


#[test]
fn fulfiller() {
    ::gj::EventLoop::init();
    let (promise, mut fulfiller) = ::gj::new_promise_and_fulfiller::<u32>();
    let p1 = promise.map(|x| {
        assert_eq!(x, 10);
        return Ok(x + 1);
    });

    fulfiller.fulfill(10);
    let value = p1.wait().unwrap();
    assert_eq!(value, 11);

}


