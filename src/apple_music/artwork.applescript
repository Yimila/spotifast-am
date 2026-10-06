-- Writes a library song's first artwork to a file and prints its format,
-- or "none". AppleScript, not JavaScript: only AppleScript can write the
-- raw picture data Music.app hands back.
on run argv
	set trackID to item 1 of argv
	set outPath to item 2 of argv
	tell application "Music"
		set found to (every track of library playlist 1 whose persistent ID is trackID)
		if (count of found) is 0 then return "none"
		set theTrack to item 1 of found
		if (count of artworks of theTrack) is 0 then return "none"
		set pictureData to raw data of artwork 1 of theTrack
		set pictureFormat to format of artwork 1 of theTrack as text
	end tell
	set outFile to open for access (POSIX file outPath) with write permission
	try
		set eof outFile to 0
		write pictureData to outFile
		close access outFile
	on error message
		close access outFile
		error message
	end try
	return pictureFormat
end run
