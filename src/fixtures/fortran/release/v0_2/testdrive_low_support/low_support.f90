! Bounded low-support fixture: two admitted anchors, one below the family's
! minimum support of three.
module low_support
  use testdrive, only : error_type, check
  implicit none

contains

  subroutine test_only_one(error)
    type(error_type), allocatable, intent(out) :: error
    call check(error, 1 == 1)
    if (allocated(error)) return
  end subroutine test_only_one

  subroutine test_only_two(error)
    type(error_type), allocatable, intent(out) :: error
    call check(error, 2 == 2)
    if (allocated(error)) return
  end subroutine test_only_two

end module low_support
