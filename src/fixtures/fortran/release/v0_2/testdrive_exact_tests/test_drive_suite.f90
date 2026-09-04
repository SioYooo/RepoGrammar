! Bounded positive fixture: the canonical test-drive module suite.
! Mirrors the framework README shape: a module-private test set behind a
! public collect subroutine, with three distinct admitted anchors.
module test_drive_suite
  use testdrive, only : new_unittest, unittest_type, error_type, check, test_failed
  implicit none
  private

  public :: collect_suite

contains

  subroutine collect_suite(testsuite)
    type(unittest_type), allocatable, intent(out) :: testsuite(:)

    testsuite = [ &
      new_unittest("plain", test_plain), &
      new_unittest("multi", test_multi), &
      new_unittest("alloc", test_alloc_order, should_fail=.false.) &
    ]

  end subroutine collect_suite

  subroutine test_plain(error)
    type(error_type), allocatable, intent(out) :: error

    call check(error, 1 + 2 == 3)
    if (allocated(error)) return

  end subroutine test_plain

  subroutine test_multi(error, first, last)
    type(error_type), allocatable, intent(out) :: error
    integer, intent(in) :: first, last

    if (first > last) then
      call test_failed(error, "bounds inverted")
      return
    end if

  end subroutine test_multi

  subroutine test_alloc_order(error)
    type(error_type), intent(out), allocatable :: error

    call check(error, 2 == 2)
    if (allocated(error)) return

  end subroutine test_alloc_order

end module test_drive_suite
