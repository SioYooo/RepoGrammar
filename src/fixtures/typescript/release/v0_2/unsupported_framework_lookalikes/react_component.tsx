import React from "react";

export function UserCard() {
  return <section>user</section>;
}

export function useUser() {
  return React.useMemo(() => ({ id: 1 }), []);
}
