#include <cstdint>
#include <iostream>
#include <optional>
#include <ostream>

template<typename T, typename I, typename F>
class FilterIter;
template<typename O, typename N, typename F>
class MapIter;
template<typename T>
class OnceIter;

#define ITER_BODY(T, I)                                         \
    template<typename F1>                                       \
    FilterIter<I, T, F1> filter(F1 filter) {                    \
        FilterIter<I, T, F1> iter(*this, filter);               \
        return iter;                                            \
    }                                                           \
    template<typename N1, typename F1>                          \
    MapIter<I, N1, F1> map(F1 filter) {                         \
        MapIter<I, N1, F1> iter(*this, filter);                 \
        return iter;                                            \
    }

#define ITER_STATICS(T)                                         \
    static OnceIter<T> once(T item) {                           \
        OnceIter<T> iter(item);                                 \
        return iter;                                            \
    }

template<typename T>
class Iterator {
public:
    virtual std::optional<T> next();

    ITER_STATICS(T)
};

template<typename I, typename N, typename F>
class MapIter : public Iterator<N> {
    I inner;
    F map_fn;

public:
    std::optional<N> next() {
        auto value = inner.next();
        if (value.has_value()) return map_fn(value.value());
        return std::nullopt;
    }

    MapIter(I inner, F map) : map_fn(map), inner(inner) {}

    using Iter = MapIter<I, N, F>;
    ITER_BODY(N, Iter);
};

template<typename T>
class OnceIter : public Iterator<T> {
    std::optional<T> item;
public:
    OnceIter(T item) : item(item) {}

    std::optional<T> next();

    ITER_BODY(T, OnceIter<T>);
};

template<typename T>
std::optional<T> OnceIter<T>::next() {
    std::optional<T> v;
    v.swap(item);
    return v;
}

template<typename I, typename T, typename F>
class FilterIter : public Iterator<T> {
    I inner;
    F filter_fn;

public:
    std::optional<T> next() {
        while(auto value = inner.next()) {
            if(filter_fn(value.value())) return value;
        }
        return std::nullopt;
    }
    FilterIter(Iterator<T> inner, F filter) : filter_fn(filter), inner(inner) {}

    using Iter = FilterIter<T, I, F>;
    ITER_BODY(T, Iter);
};

template<typename T>
class Meower;

template<typename T>
class Human {
public:
    void say(T thing);

    static Meower<T> meower() {
        Meower<T> meower;
        return meower;
    }
};

template<typename T>
class Meower : public Human<T> {
public:
    void say(T thing);
};

template<typename T>
void Meower<T>::say(T thing) {
    std::cout << thing << std::endl;
}

int main() {
    // auto v = Iterator<int32_t>::once(12).next();//.map<int32_t>([](int32_t v) { return v * 2; }).next();//.filter([](int32_t v){return v > 0;}).next();
    // std::cout << v.value_or(-1200000) << std::endl;
    Human<int>::meower().say(12);
}
